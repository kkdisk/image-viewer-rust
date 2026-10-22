//! 局部放大鏡視窗與像素資訊檢視元件 (Magnifier HUD)

use crate::theme::ThemeColors;
use egui::{Color32, FontId, Pos2, Rect, Rounding, Stroke, Ui, pos2, vec2};

/// 放大鏡渲染器
pub struct MagnifierWidget;

impl MagnifierWidget {
    /// 繪製局部放大鏡浮動視窗
    ///
    /// - `ui`: 當前 egui Ui
    /// - `viewport_rect`: 視口畫布的可視範圍
    /// - `image_rect`: 影像在畫布上的實際繪製區域
    /// - `image`: 當前完整 RGBA 像素影像緩衝
    /// - `zoom`: 放大倍率 (2, 4, 8)
    /// - `cursor_pos`: 游標在視窗中的絕對位置
    pub fn render(
        ui: &mut Ui,
        viewport_rect: Rect,
        image_rect: Rect,
        image: &image::RgbaImage,
        zoom: u32,
        cursor_pos: Pos2,
    ) {
        // 判斷游標是否落在圖片繪製區域內
        if !image_rect.contains(cursor_pos)
            || image_rect.width() <= 0.0
            || image_rect.height() <= 0.0
        {
            return;
        }

        let img_w = image.width();
        let img_h = image.height();
        if img_w == 0 || img_h == 0 {
            return;
        }

        // 換算游標在原始影像的像素座標
        let rel_x = (cursor_pos.x - image_rect.min.x) / image_rect.width();
        let rel_y = (cursor_pos.y - image_rect.min.y) / image_rect.height();
        let px = (rel_x * img_w as f32).floor() as i64;
        let py = (rel_y * img_h as f32).floor() as i64;

        if px < 0 || px >= img_w as i64 || py < 0 || py >= img_h as i64 {
            return;
        }

        // 浮動視窗幾何尺寸
        let window_w = 196.0;
        let grid_area_size = 176.0;
        let window_h = 248.0;

        // 動態計算浮動視窗位置（避免超出視口右方或下方）
        let offset = vec2(20.0, 20.0);
        let mut win_min = cursor_pos + offset;

        if win_min.x + window_w > viewport_rect.max.x - 8.0 {
            win_min.x = cursor_pos.x - window_w - offset.x;
        }
        if win_min.y + window_h > viewport_rect.max.y - 8.0 {
            win_min.y = cursor_pos.y - window_h - offset.y;
        }

        // 視口外邊界防禦 clamp
        let min_x = viewport_rect.min.x + 4.0;
        let min_y = viewport_rect.min.y + 4.0;
        win_min.x = win_min
            .x
            .clamp(min_x, (viewport_rect.max.x - window_w - 4.0).max(min_x));
        win_min.y = win_min
            .y
            .clamp(min_y, (viewport_rect.max.y - window_h - 4.0).max(min_y));

        let window_rect = Rect::from_min_size(win_min, vec2(window_w, window_h));
        let painter = ui.painter().with_clip_rect(viewport_rect);

        // 1. 繪製浮動視窗外框與背景陰影
        painter.rect_filled(
            window_rect,
            Rounding::same(8.0),
            Color32::from_black_alpha(235),
        );
        painter.rect_stroke(
            window_rect,
            Rounding::same(8.0),
            Stroke::new(1.0, ThemeColors::BORDER_FOCUS),
        );

        // 2. 視窗頂部標題列
        let title_pos = pos2(window_rect.min.x + 10.0, window_rect.min.y + 7.0);
        let title_text = format!("🔍 放大鏡 {}x  ({}, {})", zoom, px, py);
        painter.text(
            title_pos,
            egui::Align2::LEFT_TOP,
            title_text,
            FontId::proportional(11.0),
            ThemeColors::TEXT_PRIMARY,
        );

        // 3. 像素網格繪製
        let grid_min = pos2(window_rect.min.x + 10.0, window_rect.min.y + 24.0);
        let radius: i32 = match zoom {
            2 => 7,
            8 => 3,
            _ => 5, // 4x (預設 11x11 格)
        };
        let grid_count = radius * 2 + 1;
        let cell_size = grid_area_size / grid_count as f32;

        let grid_rect = Rect::from_min_size(grid_min, vec2(grid_area_size, grid_area_size));
        painter.rect_filled(
            grid_rect,
            Rounding::same(2.0),
            Color32::from_rgb(18, 18, 22),
        );

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let sx = px + dx as i64;
                let sy = py + dy as i64;

                let cell_x = grid_min.x + (dx + radius) as f32 * cell_size;
                let cell_y = grid_min.y + (dy + radius) as f32 * cell_size;
                let cell_rect =
                    Rect::from_min_size(pos2(cell_x, cell_y), vec2(cell_size, cell_size));

                let color = if sx >= 0 && sx < img_w as i64 && sy >= 0 && sy < img_h as i64 {
                    let pixel = image.get_pixel(sx as u32, sy as u32);
                    Color32::from_rgba_unmultiplied(pixel[0], pixel[1], pixel[2], pixel[3])
                } else {
                    // 超出影像邊界時以深灰網格表示
                    Color32::from_rgb(26, 26, 32)
                };

                painter.rect_filled(cell_rect, 0.0, color);

                // 微弱網格線
                if cell_size >= 12.0 {
                    painter.rect_stroke(
                        cell_rect,
                        0.0,
                        Stroke::new(0.5, Color32::from_black_alpha(70)),
                    );
                }

                // 中心像素十字焦點高亮標記
                if dx == 0 && dy == 0 {
                    painter.rect_stroke(
                        cell_rect,
                        0.0,
                        Stroke::new(2.0, Color32::from_rgb(0, 229, 255)),
                    );

                    // 準星十字線
                    let center_pt = cell_rect.center();
                    let cross_len = (cell_size * 0.35).min(5.0);
                    painter.line_segment(
                        [
                            pos2(center_pt.x - cross_len, center_pt.y),
                            pos2(center_pt.x + cross_len, center_pt.y),
                        ],
                        Stroke::new(1.0, Color32::WHITE),
                    );
                    painter.line_segment(
                        [
                            pos2(center_pt.x, center_pt.y - cross_len),
                            pos2(center_pt.x, center_pt.y + cross_len),
                        ],
                        Stroke::new(1.0, Color32::WHITE),
                    );
                }
            }
        }

        // 外框網格描邊
        painter.rect_stroke(
            grid_rect,
            Rounding::same(2.0),
            Stroke::new(1.0, Color32::from_white_alpha(40)),
        );

        // 4. 底部色彩資訊列
        let info_y = grid_min.y + grid_area_size + 8.0;
        let center_pixel = image.get_pixel(px as u32, py as u32);
        let r = center_pixel[0];
        let g = center_pixel[1];
        let b = center_pixel[2];
        let a = center_pixel[3];

        // 色彩預覽小色塊
        let swatch_rect = Rect::from_min_size(
            pos2(window_rect.min.x + 10.0, info_y + 1.0),
            vec2(22.0, 22.0),
        );
        let swatch_color = Color32::from_rgba_unmultiplied(r, g, b, a);
        painter.rect_filled(swatch_rect, Rounding::same(3.0), swatch_color);
        painter.rect_stroke(
            swatch_rect,
            Rounding::same(3.0),
            Stroke::new(1.0, Color32::from_white_alpha(100)),
        );

        // 顏色 Hex 碼與 RGBA 標籤
        let hex_text = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let hex_pos = pos2(swatch_rect.max.x + 8.0, info_y);
        painter.text(
            hex_pos,
            egui::Align2::LEFT_TOP,
            hex_text,
            FontId::monospace(11.0),
            ThemeColors::TEXT_PRIMARY,
        );

        let rgba_text = if a == 255 {
            format!("RGB: {}, {}, {}", r, g, b)
        } else {
            format!("RGBA: {}, {}, {}, {}", r, g, b, a)
        };
        let rgba_pos = pos2(swatch_rect.max.x + 8.0, info_y + 13.0);
        painter.text(
            rgba_pos,
            egui::Align2::LEFT_TOP,
            rgba_text,
            FontId::proportional(10.0),
            ThemeColors::TEXT_MUTED,
        );
    }
}
