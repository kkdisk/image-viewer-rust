use egui::{menu, Margin, RichText, SidePanel, TopBottomPanel};
use crate::theme::ThemeColors;

/// 頂部主選單列
pub struct MenuBar;

impl MenuBar {
    pub fn show(
        ctx: &egui::Context,
        on_open_file: &mut impl FnMut(),
        on_save_file: &mut impl FnMut(),
        show_side_panel: &mut bool,
        show_filmstrip: &mut bool,
        do_reset_100: &mut bool,
        do_reset_fit: &mut bool,
    ) {
        TopBottomPanel::top("top_menu_bar")
            .frame(egui::Frame::none().fill(ThemeColors::BG_PANEL).inner_margin(Margin::symmetric(10.0, 4.0)))
            .show(ctx, |ui| {
                menu::bar(ui, |ui| {
                    ui.menu_button("檔案 (File)", |ui| {
                        if ui.button("📂 開啟圖片 (Open)...").clicked() {
                            on_open_file();
                            ui.close_menu();
                        }
                        if ui.button("💾 儲存變更 (Save)").clicked() {
                            on_save_file();
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("❌ 結束 (Exit)").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button("檢視 (View)", |ui| {
                        ui.checkbox(show_side_panel, "側邊微調面板");
                        ui.checkbox(show_filmstrip, "底部縮圖列表 (Filmstrip)");
                        ui.separator();
                        if ui.button("🔍 100% 原始尺寸 (1)").clicked() {
                            *do_reset_100 = true;
                            ui.close_menu();
                        }
                        if ui.button("📐 符合視窗 (0 / F)").clicked() {
                            *do_reset_fit = true;
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("工具 (Tools)", |ui| {
                        if ui.button("👥 重複圖片檢查器 (Duplicate Finder)").clicked() {
                            ui.close_menu();
                        }
                    });

                    ui.menu_button("說明 (Help)", |ui| {
                        if ui.button("ℹ 關於 Rust Image Viewer").clicked() {
                            ui.close_menu();
                        }
                    });
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
    ) {
        TopBottomPanel::bottom("bottom_status_bar")
            .frame(egui::Frame::none().fill(ThemeColors::BG_PANEL).inner_margin(Margin::symmetric(12.0, 4.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
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
                    ui.label(RichText::new(name_str).strong().color(ThemeColors::TEXT_PRIMARY));

                    // 存檔狀態標記
                    if is_dirty {
                        ui.label(RichText::new("● 未存檔").color(ThemeColors::WARNING).size(11.0));
                    } else if filename.is_some() {
                        ui.label(RichText::new("● 已存檔").color(ThemeColors::SUCCESS).size(11.0));
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

/// 側邊微調面板容器
pub struct AdjustmentPanel;

impl AdjustmentPanel {
    pub fn show(
        ctx: &egui::Context,
        is_open: &mut bool,
        has_image: bool,
        params: &mut AdjustmentParams,
        is_computing: bool,
        action: &mut Option<AdjustmentAction>,
    ) {
        if !*is_open {
            return;
        }

        SidePanel::right("side_adjustment_panel")
            .resizable(true)
            .default_width(300.0)
            .min_width(240.0)
            .max_width(420.0)
            .frame(egui::Frame::none().fill(ThemeColors::BG_PANEL).inner_margin(Margin::same(12.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎨 影像微調").strong().size(15.0).color(ThemeColors::TEXT_PRIMARY));

                    if is_computing {
                        ui.label(RichText::new("⚡ 運算中...").size(11.0).color(ThemeColors::WARNING));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✖").clicked() {
                            *is_open = false;
                        }
                    });
                });

                ui.separator();
                ui.add_space(8.0);

                if !has_image {
                    ui.vertical_centered(|ui| {
                        ui.add_space(30.0);
                        ui.label(RichText::new("尚未開啟圖片\n請先開啟一張圖片後方可進行微調").color(ThemeColors::TEXT_MUTED).size(13.0));
                    });
                    return;
                }

                let mut changed = false;

                // 1. 白平衡區塊
                ui.collapsing(RichText::new("白平衡 (White Balance)").strong().color(ThemeColors::TEXT_PRIMARY), |ui| {
                    ui.add_space(4.0);
                    let s_temp = ui.add(
                        egui::Slider::new(&mut params.temperature, -100..=100)
                            .clamping(egui::SliderClamping::Always)
                            .text("色溫 (K)"),
                    );
                    if s_temp.changed() {
                        changed = true;
                    }

                    let s_tint = ui.add(
                        egui::Slider::new(&mut params.tint, -100..=100)
                            .clamping(egui::SliderClamping::Always)
                            .text("色調 (Tint)"),
                    );
                    if s_tint.changed() {
                        changed = true;
                    }
                });

                ui.add_space(6.0);

                // 2. 細部微調區塊
                ui.collapsing(RichText::new("細部調整 (Fine Tune)").strong().color(ThemeColors::TEXT_PRIMARY), |ui| {
                    ui.add_space(4.0);
                    let s_bright = ui.add(
                        egui::Slider::new(&mut params.brightness, 0.0..=2.0)
                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0))
                            .text("亮度 (Brightness)"),
                    );
                    if s_bright.changed() {
                        changed = true;
                    }

                    let s_contrast = ui.add(
                        egui::Slider::new(&mut params.contrast, 0.0..=2.0)
                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0))
                            .text("對比度 (Contrast)"),
                    );
                    if s_contrast.changed() {
                        changed = true;
                    }

                    let s_sat = ui.add(
                        egui::Slider::new(&mut params.saturation, 0.0..=2.0)
                            .custom_formatter(|n, _| format!("{:.0}%", n * 100.0))
                            .text("飽和度 (Saturation)"),
                    );
                    if s_sat.changed() {
                        changed = true;
                    }
                });

                if changed {
                    *action = Some(AdjustmentAction::Change);
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // 3. 動作按鈕群
                let is_dirty = !params.is_identity();
                ui.horizontal(|ui| {
                    if ui.add_enabled(is_dirty, egui::Button::new("重設 (Reset)")).clicked() {
                        *action = Some(AdjustmentAction::Reset);
                    }

                    if ui.add_enabled(is_dirty, egui::Button::new("取消 (Cancel)")).clicked() {
                        *action = Some(AdjustmentAction::Cancel);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("✓ 套用 (Apply)").strong().color(egui::Color32::WHITE),
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

use egui::{pos2, vec2, Align, Rect, Rounding, ScrollArea, Stroke};
use crate::playlist::ImagePlaylist;
use crate::thumbnail_loader::ThumbnailManager;

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
            .frame(egui::Frame::none().fill(ThemeColors::BG_APP).inner_margin(Margin::symmetric(10.0, 6.0)))
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

                    ui.label(RichText::new(count_text).color(ThemeColors::TEXT_SECONDARY).size(12.0));

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
                                let (rect, response) = ui.allocate_exact_size(card_size, egui::Sense::click());

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
                                ui.painter().rect_filled(rect, Rounding::same(4.0), bg_color);

                                // 取得或請求縮圖
                                if let Some(tex) = thumbnail_mgr.get_or_request(path) {
                                    let inner_margin = 4.0;
                                    let img_rect = Rect::from_min_max(
                                        pos2(rect.left() + inner_margin, rect.top() + inner_margin),
                                        pos2(rect.right() - inner_margin, rect.bottom() - inner_margin),
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
                                ui.painter().rect_stroke(rect, Rounding::same(4.0), border_stroke);

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
