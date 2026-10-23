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
    ImportPreset,
    ExportPreset,
    Crop,
    Resize,
    Export,
    Sampling(crate::preferences::SamplingMode),
    Refresh,
    Delete,
    Slideshow,
    SlideInterval(u64),
    SlideLoop,
    SlideRandom,
    ConfirmDelete,
    Info,
    Transform(crate::convenience::Transform),
    CopyImage,
    CopyPath,
    OpenFolder,
    OpenRecent(usize),
    Sort(crate::playlist::SortOptions),
    Fullscreen,
    Shortcuts,
    Exit,
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
    pub can_copy_image: bool,
    pub sampling: crate::preferences::SamplingMode,
    pub slideshow_seconds: u64,
    pub slideshow_loop: bool,
    pub slideshow_random: bool,
    pub confirm_delete: bool,
    pub recent: &'a [crate::recent::RecentEntry],
    pub sort: crate::playlist::SortOptions,
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
                    Stroke::new(1.0_f32, Color32::from_rgb(235, 235, 242));
                menu_style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(45, 45, 52);
                menu_style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                menu_style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
                menu_style.visuals.widgets.open.bg_fill = ThemeColors::ACCENT;
                menu_style.visuals.widgets.open.bg_stroke = Stroke::NONE;
                menu_style.visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
                menu_style.spacing.button_padding = egui::vec2(8.0, 4.0);

                menu::bar(ui, |ui| {
                    ui.menu_button("檔案", |ui| {
                        if ui.button("📂 開啟圖片 (Open)...       Ctrl+O").clicked() {
                            triggered_action = Some(MenuAction::Open);
                            ui.close_menu();
                        }
                        if ui.button("開啟資料夾…").clicked() {
                            triggered_action = Some(MenuAction::OpenFolder);
                            ui.close_menu();
                        }
                        ui.menu_button("最近開啟", |ui| {
                            if state.recent.is_empty() {
                                ui.label("尚無最近項目");
                            }
                            for (index, entry) in state.recent.iter().enumerate() {
                                let label = entry
                                    .path
                                    .file_name()
                                    .unwrap_or(entry.path.as_os_str())
                                    .to_string_lossy();
                                if ui
                                    .button(label)
                                    .on_hover_text(entry.path.display().to_string())
                                    .clicked()
                                {
                                    triggered_action = Some(MenuAction::OpenRecent(index));
                                    ui.close_menu();
                                }
                            }
                        });
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
                        if ui.button("匯出副本…").clicked() {
                            triggered_action = Some(MenuAction::Export);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("❌ 結束 (Exit)").clicked() {
                            triggered_action = Some(MenuAction::Exit);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("編輯", |ui| {
                        if ui.button("裁切圖片…").clicked() {
                            triggered_action = Some(MenuAction::Crop);
                            ui.close_menu();
                        }
                        if ui.button("變更圖片尺寸…").clicked() {
                            triggered_action = Some(MenuAction::Resize);
                            ui.close_menu();
                        }
                        for (label, transform) in [
                            ("順時針旋轉 90 度", crate::convenience::Transform::Clockwise),
                            (
                                "逆時針旋轉 90 度",
                                crate::convenience::Transform::Counterclockwise,
                            ),
                            ("水平翻轉", crate::convenience::Transform::Horizontal),
                            ("垂直翻轉", crate::convenience::Transform::Vertical),
                        ] {
                            if ui.button(label).clicked() {
                                triggered_action = Some(MenuAction::Transform(transform));
                                ui.close_menu();
                            }
                        }
                        if ui
                            .add_enabled(
                                state.can_copy_image,
                                egui::Button::new("複製圖片       Ctrl+C"),
                            )
                            .on_disabled_hover_text(
                                "請先開啟圖片、套用或取消預覽，並等待目前工作完成",
                            )
                            .on_hover_text("複製已套用編輯的完整解析度影像，可貼到聊天或文件")
                            .clicked()
                        {
                            triggered_action = Some(MenuAction::CopyImage);
                            ui.close_menu();
                        }
                        if ui.button("複製來源路徑       Ctrl+Shift+C").clicked() {
                            triggered_action = Some(MenuAction::CopyPath);
                            ui.close_menu();
                        }
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

                    ui.menu_button("檢視", |ui| {
                        ui.menu_button("縮放取樣", |ui| {
                            for mode in [
                                crate::preferences::SamplingMode::Fast,
                                crate::preferences::SamplingMode::HighQuality,
                                crate::preferences::SamplingMode::Pixel,
                            ] {
                                if ui
                                    .selectable_label(state.sampling == mode, mode.label())
                                    .clicked()
                                {
                                    triggered_action = Some(MenuAction::Sampling(mode));
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button("重新整理並重載（F5）").clicked() {
                            triggered_action = Some(MenuAction::Refresh);
                            ui.close_menu();
                        }
                        if ui.button("開始／停止幻燈片（空白鍵）").clicked() {
                            triggered_action = Some(MenuAction::Slideshow);
                            ui.close_menu();
                        }
                        ui.menu_button("幻燈片選項", |ui| {
                            for seconds in [2, 3, 5, 10] {
                                if ui
                                    .selectable_label(
                                        state.slideshow_seconds == seconds,
                                        format!("{seconds} 秒"),
                                    )
                                    .clicked()
                                {
                                    triggered_action = Some(MenuAction::SlideInterval(seconds));
                                }
                            }
                            if ui
                                .selectable_label(state.slideshow_loop, "循環播放")
                                .clicked()
                            {
                                triggered_action = Some(MenuAction::SlideLoop);
                            }
                            if ui
                                .selectable_label(state.slideshow_random, "隨機順序")
                                .clicked()
                            {
                                triggered_action = Some(MenuAction::SlideRandom);
                            }
                        });
                        if ui.button("移至回收桶（Delete）").clicked() {
                            triggered_action = Some(MenuAction::Delete);
                            ui.close_menu();
                        }
                        if ui
                            .selectable_label(state.confirm_delete, "刪除前確認")
                            .clicked()
                        {
                            triggered_action = Some(MenuAction::ConfirmDelete);
                        }
                        if ui.button("圖片資訊（唯讀）").clicked() {
                            triggered_action = Some(MenuAction::Info);
                            ui.close_menu();
                        }
                        if ui.button("全螢幕（F11）").clicked() {
                            triggered_action = Some(MenuAction::Fullscreen);
                            ui.close_menu();
                        }
                        ui.menu_button("排序", |ui| {
                            for (mode, label) in [
                                (crate::playlist::SortMode::Natural, "自然檔名"),
                                (
                                    crate::playlist::SortMode::Captured,
                                    "拍攝時間（相機當地時間）",
                                ),
                                (crate::playlist::SortMode::Modified, "修改日期"),
                                (crate::playlist::SortMode::Size, "檔案大小"),
                            ] {
                                if ui
                                    .selectable_label(state.sort.mode == mode, label)
                                    .clicked()
                                {
                                    triggered_action =
                                        Some(MenuAction::Sort(crate::playlist::SortOptions {
                                            mode,
                                            ..state.sort
                                        }));
                                    ui.close_menu();
                                }
                            }
                            let mut descending = state.sort.descending;
                            if ui.checkbox(&mut descending, "降冪").changed() {
                                triggered_action =
                                    Some(MenuAction::Sort(crate::playlist::SortOptions {
                                        descending,
                                        ..state.sort
                                    }));
                            }
                        });
                        ui.checkbox(state.show_side_panel, "調整／資訊側欄");
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

                    ui.menu_button("工具", |ui| {
                        if ui.button("匯入調色預設集…").clicked() {
                            triggered_action = Some(MenuAction::ImportPreset);
                            ui.close_menu();
                        }
                        if ui.button("匯出調色預設集…").clicked() {
                            triggered_action = Some(MenuAction::ExportPreset);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui
                            .button("👥 重複圖片檢查器 (Duplicate Finder)...")
                            .clicked()
                        {
                            triggered_action = Some(MenuAction::DuplicateFinder);
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("說明", |ui| {
                        if ui.button("快捷鍵說明（F1）").clicked() {
                            triggered_action = Some(MenuAction::Shortcuts);
                            ui.close_menu();
                        }
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
                    .stroke(Stroke::new(1.0_f32, ThemeColors::BORDER))
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
                        RichText::new(concat!(
                            "版本 v",
                            env!("CARGO_PKG_VERSION"),
                            " (Rust 2024 Edition)"
                        ))
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

pub mod curve_editor;
pub mod status_bar;
pub use status_bar::{DocumentStatus, StatusBar, StatusBarState, ZoomAction};

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
    pub white_balance_open: &'a mut bool,
    pub fine_tune_open: &'a mut bool,
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
                Self::contents(ui, state, action);
            });
    }

    pub fn contents(
        ui: &mut egui::Ui,
        state: &mut AdjustmentPanelState<'_>,
        action: &mut Option<AdjustmentAction>,
    ) {
        if !state.has_image {
            ui.vertical_centered(|ui| {
                ui.add_space(30.0);
                ui.label(
                    RichText::new("尚未開啟圖片\n請先開啟一張圖片後方可進行微調與直方圖分析")
                        .color(ThemeColors::TEXT_MUTED)
                        .size(13.0),
                );
            });
            return;
        }

        let mut changed = false;
        egui::ScrollArea::vertical()
            .id_salt("adjustment_controls_scroll")
            .max_height((ui.available_height() - 86.0).max(0.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // 即時分佈直方圖
                HistogramWidget::show(ui, state.histogram_data, state.histogram_mode);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                // 1. 白平衡區塊
                let white_balance = egui::CollapsingHeader::new(
                    RichText::new("白平衡")
                        .strong()
                        .color(ThemeColors::TEXT_PRIMARY),
                )
                .id_salt("white_balance")
                .open(Some(*state.white_balance_open))
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    let s_temp = ui
                        .horizontal(|ui| {
                            ui.set_min_height(28.0);
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 120.0).max(40.0);
                            ui.add_sized([44.0, 24.0], egui::Label::new("色溫"));
                            ui.add(
                                egui::Slider::new(&mut state.params.temperature, -100..=100)
                                    .clamping(egui::SliderClamping::Always),
                            )
                        })
                        .inner;
                    if s_temp.changed() {
                        changed = true;
                    }

                    let s_tint = ui
                        .horizontal(|ui| {
                            ui.set_min_height(28.0);
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 120.0).max(40.0);
                            ui.add_sized([44.0, 24.0], egui::Label::new("色調"));
                            ui.add(
                                egui::Slider::new(&mut state.params.tint, -100..=100)
                                    .clamping(egui::SliderClamping::Always),
                            )
                        })
                        .inner;
                    if s_tint.changed() {
                        changed = true;
                    }
                });
                if white_balance.header_response.clicked() {
                    *state.white_balance_open = !*state.white_balance_open;
                }

                ui.add_space(6.0);

                // 2. 細部微調區塊
                let fine_tune = egui::CollapsingHeader::new(
                    RichText::new("細部調整")
                        .strong()
                        .color(ThemeColors::TEXT_PRIMARY),
                )
                .id_salt("fine_tune")
                .open(Some(*state.fine_tune_open))
                .show(ui, |ui| {
                    ui.add_space(4.0);
                    let s_bright = ui
                        .horizontal(|ui| {
                            ui.set_min_height(28.0);
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 120.0).max(40.0);
                            ui.add_sized([44.0, 24.0], egui::Label::new("亮度"));
                            ui.add(
                                egui::Slider::new(&mut state.params.brightness, 0.0..=2.0)
                                    .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                            )
                        })
                        .inner;
                    if s_bright.changed() {
                        changed = true;
                    }

                    let s_contrast = ui
                        .horizontal(|ui| {
                            ui.set_min_height(28.0);
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 120.0).max(40.0);
                            ui.add_sized([44.0, 24.0], egui::Label::new("對比"));
                            ui.add(
                                egui::Slider::new(&mut state.params.contrast, 0.0..=2.0)
                                    .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                            )
                        })
                        .inner;
                    if s_contrast.changed() {
                        changed = true;
                    }

                    let s_sat = ui
                        .horizontal(|ui| {
                            ui.set_min_height(28.0);
                            ui.spacing_mut().slider_width =
                                (ui.available_width() - 120.0).max(40.0);
                            ui.add_sized([44.0, 24.0], egui::Label::new("飽和度"));
                            ui.add(
                                egui::Slider::new(&mut state.params.saturation, 0.0..=2.0)
                                    .custom_formatter(|n, _| format!("{:.0}%", n * 100.0)),
                            )
                        })
                        .inner;
                    if s_sat.changed() {
                        changed = true;
                    }
                });
                if fine_tune.header_response.clicked() {
                    *state.fine_tune_open = !*state.fine_tune_open;
                }
                ui.add_space(6.0);
                egui::CollapsingHeader::new("色調曲線")
                    .id_salt("tone_curves")
                    .show(ui, |ui| {
                        changed |= curve_editor::show(ui, &mut state.params.curves);
                    });
            });

        if changed {
            *action = Some(AdjustmentAction::Change);
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(4.0);

        // 3. 動作按鈕群
        let is_dirty = !state.params.is_identity();
        ui.label(
            RichText::new(if state.is_computing {
                "預覽運算中，完成後即可套用"
            } else if is_dirty {
                "預覽尚未套用；套用後才可儲存"
            } else {
                "調整滑桿後可預覽效果"
            })
            .size(11.0)
            .color(ThemeColors::TEXT_SECONDARY),
        );
        ui.horizontal(|ui| {
            if ui
                .add_enabled(is_dirty, egui::Button::new("重設"))
                .on_hover_text("將預覽參數恢復為預設值")
                .clicked()
            {
                *action = Some(AdjustmentAction::Reset);
            }

            if ui
                .add_enabled(is_dirty, egui::Button::new("取消"))
                .on_hover_text("移除預覽，保留已套用的編輯")
                .clicked()
            {
                *action = Some(AdjustmentAction::Cancel);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let apply_btn =
                    egui::Button::new(RichText::new("套用").strong().color(egui::Color32::WHITE))
                        .fill(ThemeColors::ACCENT);

                if ui
                    .add_enabled(is_dirty && !state.is_computing, apply_btn)
                    .on_hover_text("將目前預覽加入編輯歷史，再使用 Ctrl+S 儲存")
                    .clicked()
                {
                    *action = Some(AdjustmentAction::Apply);
                }
            });
        });
    }
}

pub mod filmstrip;
pub use filmstrip::FilmstripPanel;
