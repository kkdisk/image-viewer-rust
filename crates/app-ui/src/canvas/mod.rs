//! GPU 加速之圖片視口畫布 (平移、滾輪縮放、最適視窗、雙擊重設)

use egui::{
    pos2, vec2, Color32, Rect, Response, RichText, Sense, Stroke, TextureHandle, Ui, UiBuilder,
};
use viewer_state::ViewportState;
use crate::theme::ThemeColors;

/// 中央影像畫布視圖
pub struct CanvasView;

impl CanvasView {
    /// 繪製中央畫布區域並處理互動
    pub fn show(
        ui: &mut Ui,
        texture: Option<&TextureHandle>,
        image_dims: Option<(u32, u32)>,
        viewport: &mut ViewportState,
    ) -> Response {
        let available_size = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available_size, Sense::click_and_drag());

        // 1. 繪製畫布底層背景
        ui.painter().rect_filled(rect, 0.0, ThemeColors::BG_CANVAS);

        // 2. 檢查是否有已載入之圖片紋理
        if let (Some(tex), Some((img_w_u, img_h_u))) = (texture, image_dims) {
            let img_w = img_w_u as f32;
            let img_h = img_h_u as f32;

            // 計算自適應視窗縮放比例 (Auto-fit scale)
            let fit_scale = {
                let margin = 24.0; // 邊距留白
                let available_w = (rect.width() - margin).max(10.0);
                let available_h = (rect.height() - margin).max(10.0);
                (available_w / img_w).min(available_h / img_h)
            };

            // 若目前為 auto_fit 狀態，強制採用 fit_scale
            let current_scale = if viewport.auto_fit {
                viewport.zoom = fit_scale;
                viewport.pan = (0.0, 0.0);
                fit_scale
            } else {
                viewport.zoom
            };

            // 計算目前圖片尺寸與中央座標位置
            let display_size = vec2(img_w * current_scale, img_h * current_scale);
            let center_pos = rect.center() + vec2(viewport.pan.0, viewport.pan.1);
            let image_rect = Rect::from_center_size(center_pos, display_size);

            // 繪製棋盤格透明背景襯底 (若圖片含有半透明像素)
            Self::draw_checkerboard(ui, image_rect);

            // 繪製圖片 GPU 紋理
            ui.painter().image(
                tex.id(),
                image_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );

            // 繪製優雅外邊框
            ui.painter().rect_stroke(
                image_rect,
                0.0,
                Stroke::new(1.0, Color32::from_white_alpha(35)),
            );

            // 3. 處理滑鼠互動
            // (a) 滑鼠拖曳平移 (Pan)
            if response.dragged() {
                let delta = response.drag_delta();
                viewport.pan.0 += delta.x;
                viewport.pan.1 += delta.y;
                viewport.auto_fit = false;
            }

            // (b) 滑鼠滾輪縮放 (Zoom at cursor)
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta != 0.0 && response.hovered() {
                let zoom_factor = if scroll_delta > 0.0 { 1.15 } else { 1.0 / 1.15 };
                let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());

                // 從 auto_fit 切換到自由縮放
                let base_zoom = if viewport.auto_fit {
                    viewport.auto_fit = false;
                    fit_scale
                } else {
                    viewport.zoom
                };

                let new_zoom = (base_zoom * zoom_factor).clamp(0.02, 50.0);
                let actual_factor = new_zoom / base_zoom;

                // 以游標為錨點調整平移偏移量
                viewport.pan.0 = cursor.x - rect.center().x - (cursor.x - rect.center().x - viewport.pan.0) * actual_factor;
                viewport.pan.1 = cursor.y - rect.center().y - (cursor.y - rect.center().y - viewport.pan.1) * actual_factor;
                viewport.zoom = new_zoom;
            }

            // (c) 雙擊切換：在「最適視窗」與「100% 原始大小」間切換
            if response.double_clicked() {
                if viewport.auto_fit || (viewport.zoom - fit_scale).abs() < 1e-3 {
                    viewport.reset_actual_size();
                } else {
                    viewport.reset_fit_window();
                }
            }
        } else {
            // 空狀態引導介面
            ui.allocate_new_ui(UiBuilder::new().max_rect(rect), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(rect.height() * 0.35);
                        ui.label(
                            RichText::new("🖼")
                                .size(48.0)
                                .color(ThemeColors::TEXT_MUTED),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("尚未開啟任何圖片")
                                .size(18.0)
                                .strong()
                                .color(ThemeColors::TEXT_PRIMARY),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("點擊上方選單「檔案 → 開啟」或直接將檔案拖曳至此處")
                                .size(13.0)
                                .color(ThemeColors::TEXT_SECONDARY),
                        );
                    });
                });
            });
        }

        response
    }

    /// 繪製微縮棋盤格背景（指示透明區域）
    fn draw_checkerboard(ui: &mut Ui, rect: Rect) {
        // 先繪製一層深暗基底
        ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(20, 20, 24));

        let tile_size = 16.0;
        let mut x = rect.left();
        let mut row_even = true;

        let dark_color = Color32::from_rgb(24, 24, 28);
        let light_color = Color32::from_rgb(32, 32, 38);

        while x < rect.right() {
            let mut y = rect.top();
            let mut col_even = row_even;
            let tile_w = (rect.right() - x).min(tile_size);

            while y < rect.bottom() {
                let tile_h = (rect.bottom() - y).min(tile_size);
                let color = if col_even { dark_color } else { light_color };
                ui.painter().rect_filled(
                    Rect::from_min_size(pos2(x, y), vec2(tile_w, tile_h)),
                    0.0,
                    color,
                );
                y += tile_size;
                col_even = !col_even;
            }
            x += tile_size;
            row_even = !row_even;
        }
    }
}
