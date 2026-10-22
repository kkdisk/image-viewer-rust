//! 文件狀態、檔名資訊與可操作縮放控制。

use crate::{app::ImageViewerApp, theme::ThemeColors};
use egui::{Color32, Margin, RichText, TopBottomPanel};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStatus {
    Empty,
    Saved,
    Unsaved,
    Preview {
        committed_dirty: bool,
        computing: bool,
    },
}

impl DocumentStatus {
    fn label(self) -> (&'static str, Color32) {
        match self {
            Self::Empty => ("未載入影像", ThemeColors::TEXT_MUTED),
            Self::Saved => ("已儲存", ThemeColors::SUCCESS),
            Self::Unsaved => ("編輯未儲存", ThemeColors::WARNING),
            Self::Preview {
                computing: true, ..
            } => ("預覽運算中", ThemeColors::WARNING),
            Self::Preview { .. } => ("預覽未套用", ThemeColors::WARNING),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZoomAction {
    In,
    Out,
    Fit,
    Actual,
    Set(f32),
}

pub struct StatusBarState<'a> {
    pub filename: Option<&'a str>,
    pub dimensions: Option<(u32, u32)>,
    pub zoom: f32,
    pub document: DocumentStatus,
    pub playlist_info: Option<(usize, usize)>,
    pub notification: Option<&'a str>,
}

pub struct StatusBar;

impl StatusBar {
    pub fn show(ctx: &egui::Context, state: &StatusBarState<'_>) -> Option<ZoomAction> {
        let mut action = None;
        TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                egui::Frame::none()
                    .fill(ThemeColors::BG_PANEL)
                    .inner_margin(Margin::symmetric(12.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (label, color) = state.document.label();
                    ui.label(RichText::new(label).strong().color(color));
                    ui.separator();
                    let size_text = state
                        .dimensions
                        .map(|(w, h)| format!("{w} × {h} px"))
                        .unwrap_or_default();
                    let size_width = ui
                        .painter()
                        .layout_no_wrap(
                            size_text.clone(),
                            egui::FontId::proportional(12.0),
                            ThemeColors::TEXT_SECONDARY,
                        )
                        .size()
                        .x;
                    let name = state.filename.unwrap_or("請開啟圖片");
                    let width = (ui.available_width() - size_width - 12.0).max(0.0);
                    ui.add_sized(
                        [width, 18.0],
                        egui::Label::new(RichText::new(name).color(ThemeColors::TEXT_PRIMARY))
                            .truncate(),
                    )
                    .on_hover_text(name);
                    ui.label(
                        RichText::new(size_text)
                            .size(12.0)
                            .color(ThemeColors::TEXT_SECONDARY),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    if let Some((current, total)) = state.playlist_info {
                        ui.label(format!("{} / {total}", current + 1));
                    }
                    ui.add_enabled_ui(state.document != DocumentStatus::Empty, |ui| {
                        if ui
                            .add_enabled(state.zoom > 0.02, egui::Button::new("−"))
                            .on_hover_text("縮小（−，下限 2%）")
                            .clicked()
                        {
                            action = Some(ZoomAction::Out);
                        }
                        let mut percent = state.zoom * 100.0;
                        if ui
                            .add(
                                egui::DragValue::new(&mut percent)
                                    .range(2.0..=5000.0)
                                    .clamp_existing_to_range(false)
                                    .suffix("%")
                                    .speed(1.0),
                            )
                            .on_hover_text("拖曳或點擊輸入縮放百分比")
                            .changed()
                        {
                            action = Some(ZoomAction::Set(percent / 100.0));
                        }
                        if ui
                            .add_enabled(state.zoom < 50.0, egui::Button::new("＋"))
                            .on_hover_text("放大（＋，上限 5000%）")
                            .clicked()
                        {
                            action = Some(ZoomAction::In);
                        }
                        if ui.button("100%").on_hover_text("原始尺寸（1）").clicked() {
                            action = Some(ZoomAction::Actual);
                        }
                        if ui
                            .button("符合視窗")
                            .on_hover_text("符合視窗（F 或 0）")
                            .clicked()
                        {
                            action = Some(ZoomAction::Fit);
                        }
                    });
                    if let DocumentStatus::Preview {
                        committed_dirty, ..
                    } = state.document
                    {
                        ui.label(if committed_dirty {
                            "底圖未儲存"
                        } else {
                            "底圖已儲存"
                        })
                        .on_hover_text("預覽需先套用才可儲存；取消只移除預覽");
                    }
                    ui.label(
                        RichText::new("WGPU 硬體加速")
                            .size(11.0)
                            .color(ThemeColors::TEXT_MUTED),
                    );
                });
                if let Some(message) = state.notification {
                    ui.add(
                        egui::Label::new(RichText::new(message).color(ThemeColors::ACCENT_HOVER))
                            .truncate(),
                    )
                    .on_hover_text(message);
                }
            });
        action
    }
}

impl ImageViewerApp {
    pub fn document_status(&self) -> DocumentStatus {
        let Some(session) = &self.session else {
            return DocumentStatus::Empty;
        };
        if self.is_preview_pending()
            || session.has_active_preview()
            || !self.slider_params.is_identity()
        {
            DocumentStatus::Preview {
                committed_dirty: session.is_dirty(),
                computing: self.is_preview_pending(),
            }
        } else if session.is_dirty() {
            DocumentStatus::Unsaved
        } else {
            DocumentStatus::Saved
        }
    }

    pub fn apply_zoom_action(&mut self, action: ZoomAction) {
        if self.session.is_none() || self.is_saving() || self.has_pending_leave() {
            return;
        }
        match action {
            ZoomAction::Fit => self.viewport.reset_fit_window(),
            ZoomAction::Actual => self.viewport.reset_actual_size(),
            ZoomAction::In if self.viewport.zoom < 50.0 => self.viewport.zoom_at(1.2, 0.0, 0.0),
            ZoomAction::Out if self.viewport.zoom > 0.02 => {
                self.viewport.zoom_at(1.0 / 1.2, 0.0, 0.0)
            }
            ZoomAction::In | ZoomAction::Out => {}
            ZoomAction::Set(zoom) if zoom.is_finite() && self.viewport.zoom > 0.0 => self
                .viewport
                .zoom_at(zoom.clamp(0.02, 50.0) / self.viewport.zoom, 0.0, 0.0),
            ZoomAction::Set(_) => {}
        }
    }
}
