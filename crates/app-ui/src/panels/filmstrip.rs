//! 等比例縮圖列與選取捲動管理。

use crate::playlist::ImagePlaylist;
use crate::theme::ThemeColors;
use crate::thumbnail_loader::ThumbnailManager;
use egui::{Align, Rect, ScrollArea, pos2, vec2};
use egui::{Margin, RichText, Rounding, Stroke, TopBottomPanel};

/// 底部 Filmstrip 縮圖列容器
pub struct FilmstripPanel;

fn fitted_rect(bounds: Rect, image_size: egui::Vec2) -> Rect {
    let scale = (bounds.width() / image_size.x).min(bounds.height() / image_size.y);
    Rect::from_center_size(bounds.center(), image_size * scale)
}

fn selection_changed(ctx: &egui::Context, current: Option<std::path::PathBuf>) -> bool {
    let key = egui::Id::new("filmstrip_last_selection");
    ctx.data_mut(|data| {
        let changed = data.get_temp::<Option<std::path::PathBuf>>(key).as_ref() != Some(&current);
        data.insert_temp(key, current);
        changed
    })
}

impl FilmstripPanel {
    pub fn show(
        ctx: &egui::Context,
        is_open: &mut bool,
        playlist: &ImagePlaylist,
        thumbnail_mgr: &mut ThumbnailManager,
        on_select: &mut impl FnMut(usize),
        height: &mut f32,
        on_navigate: &mut impl FnMut(isize),
    ) {
        if !*is_open {
            return;
        }
        let recenter = selection_changed(ctx, playlist.current_file().cloned());

        let panel = TopBottomPanel::bottom("bottom_filmstrip_panel")
            .resizable(true)
            .default_height(*height)
            .height_range(110.0..=220.0)
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
                            on_navigate(-1);
                        }
                        if ui.small_button("下一張 ▶").clicked() {
                            on_navigate(1);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("收合").clicked() {
                            *is_open = false;
                        }
                    });
                });

                ui.add_space(4.0);

                let card_edge = (ui.available_height() - 12.0).clamp(48.0, 164.0);
                // 橫向縮圖滾動區
                ScrollArea::horizontal()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let curr_idx = playlist.current_index();
                            let card_size = vec2(card_edge, card_edge);

                            for (idx, path) in playlist.files().iter().enumerate() {
                                let is_selected = curr_idx == Some(idx);
                                let (rect, response) =
                                    ui.allocate_exact_size(card_size, egui::Sense::click());

                                if is_selected && recenter {
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
                                    let img_rect = fitted_rect(rect.shrink(4.0), tex.size_vec2());
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
                                    Stroke::new(2.0_f32, ThemeColors::ACCENT)
                                } else if response.hovered() {
                                    Stroke::new(1.0_f32, ThemeColors::ACCENT_HOVER)
                                } else {
                                    Stroke::new(1.0_f32, ThemeColors::BORDER)
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
        if ctx.input(|i| i.pointer.primary_down() || i.pointer.any_released()) {
            *height = panel.response.rect.height().clamp(110.0, 220.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_preserve_portrait_and_landscape_proportions() {
        let bounds = Rect::from_min_size(pos2(10.0, 10.0), vec2(60.0, 60.0));
        for size in [vec2(100.0, 200.0), vec2(400.0, 100.0), vec2(100.0, 100.0)] {
            let fitted = fitted_rect(bounds, size);
            assert!(bounds.expand(0.001).contains_rect(fitted));
            assert_eq!(fitted.center(), bounds.center());
            assert!((fitted.width() / fitted.height() - size.x / size.y).abs() < 0.001);
        }
    }

    #[test]
    fn unchanged_selection_does_not_recenter_after_manual_scroll() {
        let ctx = egui::Context::default();
        let first = Some(std::path::PathBuf::from("folder/a.png"));
        assert!(selection_changed(&ctx, first.clone()));
        assert!(!selection_changed(&ctx, first));
        assert!(selection_changed(&ctx, Some("other/a.png".into())));
        assert!(selection_changed(&ctx, None));
        assert!(!selection_changed(&ctx, None));
    }
    #[test]
    fn filmstrip_dragged_height_is_restored_in_a_new_context() {
        let ctx = egui::Context::default();
        crate::theme::apply_theme(&ctx);
        let playlist = ImagePlaylist::new();
        let mut manager = ThumbnailManager::new();
        let mut height = 140.0;
        let mut frame = |ctx: &egui::Context, events| {
            let mut top = 0.0;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    FilmstripPanel::show(
                        ctx,
                        &mut true,
                        &playlist,
                        &mut manager,
                        &mut |_| {},
                        &mut height,
                        &mut |_| {},
                    );
                    top = ctx.available_rect().bottom();
                },
            );
            top
        };
        for _ in 0..3 {
            frame(&ctx, vec![]);
        }
        let top = frame(&ctx, vec![]);
        let pointer = pos2(400.0, top);
        frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(pointer),
                egui::Event::PointerButton {
                    pos: pointer,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        let moved = pointer - vec2(0.0, 40.0);
        frame(&ctx, vec![egui::Event::PointerMoved(moved)]);
        frame(&ctx, vec![]);
        frame(
            &ctx,
            vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        let resized_top = frame(&ctx, vec![]);
        assert!(resized_top < top - 20.0, "拖曳必須實際改變縮圖列高度");
        let restored = egui::Context::default();
        crate::theme::apply_theme(&restored);
        for _ in 0..3 {
            frame(&restored, vec![]);
        }
        let restored_top = frame(&restored, vec![]);
        assert!(
            (resized_top - restored_top).abs() < 1.0,
            "重建介面必須恢復拖曳後的高度"
        );
    }
}
