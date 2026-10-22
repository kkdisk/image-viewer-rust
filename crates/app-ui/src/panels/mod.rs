use crate::theme::ThemeColors;
use egui::{Color32, Margin, RichText, Rounding, SidePanel, Stroke, TopBottomPanel, menu};

pub mod duplicate_dialog;
pub use duplicate_dialog::{
    DuplicateFinderDialog, DuplicateFinderState, DuplicateGroupUi, DuplicateItemUi, DuplicateMode,
};

pub mod archive_dialog;
pub use archive_dialog::{ArchivePasswordAction, ArchivePasswordDialog};

pub mod histogram_view;
pub use histogram_view::{HistogramMode, HistogramWidget};

/// 選單觸發之動作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Reset100,
    ResetFit,
    DuplicateFinder,
    About,
}

/// 選單狀態與切換開關
pub struct MenuBarState<'a> {
    pub can_undo: bool,
    pub can_redo: bool,
    pub show_side_panel: &'a mut bool,
    pub show_filmstrip: &'a mut bool,
    pub is_side_by_side: &'a mut bool,
    pub sync_viewport: &'a mut bool,
    pub magnifier_enabled: &'a mut bool,
}

/// 頂部主選單列
pub struct MenuBar;

impl MenuBar {
    pub fn show(ctx: &egui::Context, state: &mut MenuBarState<'_>) -> Option<MenuAction> {
        let mut triggered_action = None;

        TopBottomPanel::top("top_menu_bar")
            .frame(
                egui::Frame::none()
                    .fill(ThemeColors::BG_PANEL)
                    .inner_margin(Margin::symmetric(10.0, 4.0)),
            )
            .show(ctx, |ui| {
                // 自訂頂部選單列專屬樣式：高對比清晰文字、平時透明底、平滑懸停底色
                let menu_style = ui.style_mut();
                menu_style.visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
                menu_style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
                menu_style.visuals.widgets.inactive.fg_stroke =
                    Stroke::new(1.0, Color32::from_rgb(235, 235, 242));
                menu_style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 45, 52);
                menu_style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                menu_style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
                menu_style.visuals.widgets.open.bg_fill = ThemeColors::ACCENT;
                menu_style.visuals.widgets.open.bg_stroke = Stroke::NONE;
                menu_style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);
                menu_style.spacing.button_padding = egui::vec2(8.0, 4.0);

                menu::bar(ui, |ui| {
                    ui.menu_button("檔案 (File)", |ui| {
                        if ui.button("📂 開啟圖片 (Open)...       Ctrl+O").clicked() {
                            triggered_action = Some(MenuAction::Open);
                            ui.close_menu();
                        }
                        if ui.button("💾 儲存變更 (Save)          Ctrl+S").clicked() {
                            triggered_action = Some(MenuAction::Save);
                            ui.close_menu();
                        }
                        if ui
                            .button("📝 另存新檔 (Save As)...  Ctrl+Shift+S")
                            .clicked()
                        {
                            triggered_action = Some(MenuAction::SaveAs);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("❌ 結束 (Exit)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("編輯 (Edit)", |ui| {
                        let undo_btn = ui.add_enabled(
                            state.can_undo,
                            egui::Button::new("⮌ 復原 (Undo)    Ctrl+Z"),
                        );
                        if undo_btn.clicked() {
                            triggered_action = Some(MenuAction::Undo);
                            ui.close_menu();
                        }

                        let redo_btn = ui.add_enabled(
                            state.can_redo,
                            egui::Button::new("⮎ 重做 (Redo)    Ctrl+Y"),
                        );
                        if redo_btn.clicked() {
                            triggered_action = Some(MenuAction::Redo);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("檢視 (View)", |ui| {
                        ui.checkbox(state.show_side_panel, "側邊微調面板");
                        ui.checkbox(state.show_filmstrip, "底部縮圖列表 (Filmstrip)");
                        ui.checkbox(state.magnifier_enabled, "🔎 局部放大鏡 (Magnifier)      M");
                        ui.separator();
                        ui.checkbox(
                            state.is_side_by_side,
                            "🔀 雙圖分割比對 (Side-by-Side)  Ctrl+D",
                        );
                        if *state.is_side_by_side {
                            ui.checkbox(state.sync_viewport, "  └ 🔗 同步平移與縮放");
                        }
                        ui.separator();
                        if ui.button("🔍 100% 原始尺寸 (1)").clicked() {
                            triggered_action = Some(MenuAction::Reset100);
                            ui.close_menu();
                        }
                        if ui.button("📐 符合視窗 (0 / F)").clicked() {
                            triggered_action = Some(MenuAction::ResetFit);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("工具 (Tools)", |ui| {
                        if ui
                            .button("👥 重複圖片檢查器 (Duplicate Finder)...")
                            .clicked()
                        {
                            triggered_action = Some(MenuAction::DuplicateFinder);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("說明 (Help)", |ui| {
                        if ui.button("ℹ 關於 Rust Image Viewer").clicked() {
                            triggered_action = Some(MenuAction::About);
                            ui.close_menu();
                        }
                    });
                });
            });

        triggered_action
    }
}

/// 關於對話框視窗
pub struct AboutDialog;

impl AboutDialog {
    pub fn show(ctx: &egui::Context, is_open: &mut bool) {
        if !*is_open {
            return;
        }

        egui::Window::new("ℹ 關於 Rust Image Viewer")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .fixed_size(egui::vec2(440.0, 340.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(ThemeColors::BG_PANEL)
                    .inner_margin(Margin::same(20.0))
                    .stroke(Stroke::new(1.0, ThemeColors::BORDER))
                    .rounding(Rounding::same(8.0)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("🖼").size(48.0));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Rust Image Viewer")
                            .size(20.0)
                            .strong()
                            .color(ThemeColors::TEXT_PRIMARY),
                    );
                    ui.label(
                        RichText::new("版本 v0.1.0 (Rust 2024 Edition)")
                            .size(12.0)
                            .color(ThemeColors::ACCENT),
                    );
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.label(
                    RichText::new("高效能純 Rust 桌面圖片瀏覽、編輯與查重工具：")
                        .size(13.0)
                        .strong()
                        .color(ThemeColors::TEXT_PRIMARY),
                );
                ui.add_space(6.0);

                ui.label(
                    RichText::new("• ⚡ 硬體加速：WGPU 與 egui 流暢 60+ FPS 縮放與平移畫布")
                        .size(12.0)
                        .color(ThemeColors::TEXT_SECONDARY),
                );
                ui.label(
                    RichText::new(
                        "• 🧵 平行運算：Rayon 多執行緒微調（白平衡、曝光、對比、飽和度）",
                    )
                    .size(12.0)
                    .color(ThemeColors::TEXT_SECONDARY),
                );
                ui.label(
                    RichText::new(
                        "• 🛡️ 安全防護：同目錄原子覆寫、EXIF 旋轉自動校正與 64M 像素保護",
                    )
                    .size(12.0)
                    .color(ThemeColors::TEXT_SECONDARY),
                );
                ui.label(
                    RichText::new("• 🔀 非破壞式：多步 In-Memory 復原/重做與雙圖分割同步比對視圖")
                        .size(12.0)
                        .color(ThemeColors::TEXT_SECONDARY),
                );
                ui.label(
                    RichText::new(
                        "• 👥 重複查重：多執行緒 pHash/DCT 感知雜湊與 Union-Find 自動分群",
                    )
                    .size(12.0)
                    .color(ThemeColors::TEXT_SECONDARY),
                );
                ui.label(
                    RichText::new("• 🔍 局部探針：游標局部放大鏡 (M 鍵)、RGB/Luma 即時直方圖分析")
                        .size(12.0)
                        .color(ThemeColors::TEXT_SECONDARY),
                );

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(10.0);

                ui.vertical_centered(|ui| {
                    let close_btn = egui::Button::new(
                        RichText::new("確定 (Close)")
                            .size(13.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(ThemeColors::ACCENT)
                    .min_size(egui::vec2(100.0, 28.0));

                    if ui.add(close_btn).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape))
                    {
                        *is_open = false;
                    }
                });
            });
    }
}

/// 底部狀態列
pub struct StatusBar;

impl StatusBar {
    pub fn show(
        ctx: &egui::Context,
        filename: Option<&str>,
        dimensions: Option<(u32, u32)>,
        zoom_pct: f32,
        is_dirty: bool,
        playlist_info: Option<(usize, usize)>,
        status_msg: Option<&str>,
    ) {
        TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                egui::Frame::none()
                    .fill(ThemeColors::BG_PANEL)
                    .inner_margin(Margin::symmetric(12.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // 即時狀態或通知訊息
                    if let Some(msg) = status_msg {
                        ui.label(
                            RichText::new(format!("📢 {}", msg))
                                .strong()
                                .color(ThemeColors::ACCENT_HOVER),
                        );
                        ui.separator();
                    }

                    // 檔案清單進度序號
                    if let Some((curr, total)) = playlist_info {
                        ui.label(
                            RichText::new(format!("[ {} / {} ]", curr + 1, total))
                                .strong()
                                .color(ThemeColors::ACCENT),
                        );
                        ui.separator();
                    }

                    // 檔案名稱
                    let name_str = filename.unwrap_or("未載入影像");
                    ui.label(
                        RichText::new(name_str)
                            .strong()
                            .color(ThemeColors::TEXT_PRIMARY),
                    );

                    // 存檔狀態標記
                    if is_dirty {
                        ui.label(
                            RichText::new("● 未存檔")
                                .color(ThemeColors::WARNING)
                                .size(11.0),
                        );
                    } else if filename.is_some() {
                        ui.label(
                            RichText::new("● 已存檔")
                                .color(ThemeColors::SUCCESS)
                                .size(11.0),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // 縮放百分比
                        ui.label(
                            RichText::new(format!("{:.0}%", zoom_pct * 100.0))
                                .color(ThemeColors::TEXT_SECONDARY),
                        );
                        ui.separator();

                        // 影像解析度尺寸
                        if let Some((w, h)) = dimensions {
                            ui.label(
                                RichText::new(format!("{w} × {h} px"))
                                    .color(ThemeColors::TEXT_SECONDARY),
                            );
                            ui.separator();
                        }

                        // 渲染模式標記
                        ui.label(
                            RichText::new("WGPU 硬體加速")
                                .size(11.0)
                                .color(ThemeColors::TEXT_MUTED),
                        );
                    });
                });
            });
    }
}

use core_image::AdjustmentParams;

/// 微調面板觸發之動作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustmentAction {
    Change,
    Reset,
    Cancel,
    Apply,
}

/// 側邊微調面板狀態與參數容器
pub struct AdjustmentPanelState<'a> {
    pub is_open: &'a mut bool,
    pub has_image: bool,
    pub params: &'a mut AdjustmentParams,
    pub is_computing: bool,
    pub histogram_data: Option<&'a core_image::HistogramData>,
    pub histogram_mode: &'a mut HistogramMode,
}

/// 側邊微調面板容器
pub struct AdjustmentPanel;

impl AdjustmentPanel {
    pub fn show(
        ctx: &egui::Context,
        state: &mut AdjustmentPanelState<'_>,
        action: &mut Option<AdjustmentAction>,
    ) {
        if !*state.is_open {
            return;
        }

        SidePanel::right("side_adjustment_panel")
            .resizable(true)
            .default_width(300.0)
            .min_width(240.0)
            .max_width(420.0)
            .frame(
                egui::Frame::none()
                    .fill(ThemeColors::BG_PANEL)
                    .inner_margin(Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("🎨 影像微調")
                            .strong()
                            .size(15.0)
                            .color(ThemeColors::TEXT_PRIMARY),
                    );

                    if state.is_computing {
                        ui.label(
                            RichText::new("⚡ 運算中...")
                                .size(11.0)
                                .color(ThemeColors::WARNING),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✖").clicked() {
                            *state.is_open = false;
                        }
                    });
                });

                ui.separator();
                ui.add_space(8.0);

                if !state.has_image {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(
                            RichText::new(
                                "尚未開啟圖片\n請先開啟一張圖片後方可進行微調與直方圖分析",
                            )
                            .color(ThemeColors::TEXT_MUTED)
                            .size(13.0),
                        );
                    });
                    return;
                }

                // 即時分佈直方圖
                HistogramWidget::show(ui, state.histogram_data, state.histogram_mode);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                let mut changed = false;

                // 1. 白平衡區塊
                ui.collapsing(
                    RichText::new("白平衡 (White Balance)")
                        .strong()
                        .color(ThemeColors::TEXT_PRIMARY),
                    |ui| {
                        ui.add_space(4.0);
                        let s_temp = ui.add(
                            egui::Slider::new(&mut state.params.temperature, -100..=100)
                                .clamping(egui::SliderClamping::Always)
                                .text("色溫 (K)"),
                        );
                        if s_temp.changed() {
                            changed = true;
                        }

                        let s_tint = ui.add(
                            egui::Slider::new(&mut state.params.tint, -100..=100)
                                .clamping(egui::SliderClamping::Always)
                                .text("色調 (Tint)"),
                        );
                        if s_tint.changed() {
                            changed = true;
                        }
                    },
                );

                ui.add_space(6.0);

                // 2. 細部微調區塊
                ui.collapsing(
                    RichText::new("細部調整 (Fine Tune)")
                        .strong()
                        .color(ThemeColors::TEXT_PRIMARY),
                    |ui| {
                        ui.add_space(4.0);
                        let s_bright = ui.add(
                            egui::Slider::new(&mut state.params.brightness, 0.0..=2.0)
                                .custom_formatter(|n, _| format!("{:.0}%", n * 100.0))
                                .text("亮度 (Brightness)"),
                        );
                        if s_bright.changed() {
                            changed = true;
                        }

                        let s_contrast = ui.add(
                            egui::Slider::new(&mut state.params.contrast, 0.0..=2.0)
                                .custom_formatter(|n, _| format!("{:.0}%", n * 100.0))
                                .text("對比度 (Contrast)"),
                        );
                        if s_contrast.changed() {
                            changed = true;
                        }

                        let s_sat = ui.add(
                            egui::Slider::new(&mut state.params.saturation, 0.0..=2.0)
                                .custom_formatter(|n, _| format!("{:.0}%", n * 100.0))
                                .text("飽和度 (Saturation)"),
                        );
                        if s_sat.changed() {
                            changed = true;
                        }
                    },
                );

                if changed {
                    *action = Some(AdjustmentAction::Change);
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // 3. 動作按鈕群
                let is_dirty = !state.params.is_identity();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(is_dirty, egui::Button::new("重設 (Reset)"))
                        .clicked()
                    {
                        *action = Some(AdjustmentAction::Reset);
                    }

                    if ui
                        .add_enabled(is_dirty, egui::Button::new("取消 (Cancel)"))
                        .clicked()
                    {
                        *action = Some(AdjustmentAction::Cancel);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("套用 (Apply)")
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(ThemeColors::ACCENT);

                        if ui.add_enabled(is_dirty, apply_btn).clicked() {
                            *action = Some(AdjustmentAction::Apply);
                        }
                    });
                });
            });
    }
}

use crate::playlist::ImagePlaylist;
use crate::thumbnail_loader::ThumbnailManager;
use egui::{Align, Rect, ScrollArea, pos2, vec2};

/// 底部 Filmstrip 縮圖列容器
pub struct FilmstripPanel;

impl FilmstripPanel {
    pub fn show(
        ctx: &egui::Context,
        is_open: &mut bool,
        playlist: &ImagePlaylist,
        thumbnail_mgr: &mut ThumbnailManager,
        on_select: &mut impl FnMut(usize),
        on_prev: &mut impl FnMut(),
        on_next: &mut impl FnMut(),
    ) {
        if !*is_open {
            return;
        }

        TopBottomPanel::bottom("bottom_filmstrip_panel")
            .resizable(false)
            .exact_height(105.0)
            .frame(
                egui::Frame::none()
                    .fill(ThemeColors::BG_APP)
                    .inner_margin(Margin::symmetric(10.0, 6.0)),
            )
            .show(ctx, |ui| {
                // 頂部小工具列
                ui.horizontal(|ui| {
                    let count_text = if playlist.is_empty() {
                        "🎞 縮圖列表 (無檔案)".to_owned()
                    } else if let Some(idx) = playlist.current_index() {
                        format!("🎞 縮圖列表 ({} / {})", idx + 1, playlist.len())
                    } else {
                        format!("🎞 縮圖列表 (共 {} 張)", playlist.len())
                    };

                    ui.label(
                        RichText::new(count_text)
                            .color(ThemeColors::TEXT_SECONDARY)
                            .size(12.0),
                    );

                    if !playlist.is_empty() {
                        ui.add_space(8.0);
                        if ui.small_button("◀ 上一張").clicked() {
                            on_prev();
                        }
                        if ui.small_button("下一張 ▶").clicked() {
                            on_next();
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("收合").clicked() {
                            *is_open = false;
                        }
                    });
                });

                ui.add_space(4.0);

                // 橫向縮圖滾動區
                ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let curr_idx = playlist.current_index();
                            let card_size = vec2(68.0, 68.0);

                            for (idx, path) in playlist.files().iter().enumerate() {
                                let is_selected = curr_idx == Some(idx);
                                let (rect, response) =
                                    ui.allocate_exact_size(card_size, egui::Sense::click());

                                if is_selected {
                                    response.scroll_to_me(Some(Align::Center));
                                }

                                // 背景底板
                                let bg_color = if is_selected {
                                    ThemeColors::BG_WIDGET_ACTIVE
                                } else if response.hovered() {
                                    ThemeColors::BG_WIDGET_HOVER
                                } else {
                                    ThemeColors::BG_WIDGET
                                };
                                ui.painter()
                                    .rect_filled(rect, Rounding::same(4.0), bg_color);

                                // 取得或請求縮圖：僅當可見或處於目前鄰近範圍內時優先向後台發起載入，其餘僅查快取
                                let is_visible = ui.is_rect_visible(rect);
                                let is_near = if let Some(c) = curr_idx {
                                    (idx as isize - c as isize).abs() <= 10
                                } else {
                                    idx < 15
                                };

                                let tex_opt = if is_visible || is_near {
                                    thumbnail_mgr.get_or_request(path)
                                } else {
                                    thumbnail_mgr.get_cached(path)
                                };

                                if let Some(tex) = tex_opt {
                                    let inner_margin = 4.0;
                                    let img_rect = Rect::from_min_max(
                                        pos2(rect.left() + inner_margin, rect.top() + inner_margin),
                                        pos2(
                                            rect.right() - inner_margin,
                                            rect.bottom() - inner_margin,
                                        ),
                                    );
                                    ui.painter().image(
                                        tex.id(),
                                        img_rect,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        egui::Color32::WHITE,
                                    );
                                } else {
                                    // 載入中佔位顯示
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "⏳",
                                        egui::FontId::proportional(16.0),
                                        ThemeColors::TEXT_MUTED,
                                    );
                                }

                                // 邊框高亮
                                let border_stroke = if is_selected {
                                    Stroke::new(2.0, ThemeColors::ACCENT)
                                } else if response.hovered() {
                                    Stroke::new(1.0, ThemeColors::ACCENT_HOVER)
                                } else {
                                    Stroke::new(1.0, ThemeColors::BORDER)
                                };
                                ui.painter()
                                    .rect_stroke(rect, Rounding::same(4.0), border_stroke);

                                // 點擊卡片切換
                                if response.clicked() {
                                    on_select(idx);
                                }
                            }
                        });
                    });
            });
    }
}
