//! 直方圖即時繪製元件 (RGB / 亮度分佈曲線圖)

use crate::theme::ThemeColors;
use core_image::HistogramData;
use egui::{Color32, Pos2, Response, RichText, Rounding, Sense, Stroke, Ui, pos2, vec2};

/// 直方圖通道顯示模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistogramMode {
    #[default]
    Rgb, // RGB 彩色疊加
    Red,   // 紅色通道
    Green, // 綠色通道
    Blue,  // 藍色通道
    Luma,  // 感知亮度
}

/// 直方圖繪製面板
pub struct HistogramWidget;

impl HistogramWidget {
    /// 於 UI 中渲染直方圖曲線與頻道切換按鈕
    pub fn show(ui: &mut Ui, data: Option<&HistogramData>, mode: &mut HistogramMode) -> Response {
        ui.vertical(|ui| {
            // 頂部通道切換按鈕列
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("📊 分布直方圖")
                        .strong()
                        .size(13.0)
                        .color(ThemeColors::TEXT_PRIMARY),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_luma = ui.selectable_label(*mode == HistogramMode::Luma, "亮度");
                    if btn_luma.clicked() {
                        *mode = HistogramMode::Luma;
                    }
                    let btn_b = ui.selectable_label(*mode == HistogramMode::Blue, "B");
                    if btn_b.clicked() {
                        *mode = HistogramMode::Blue;
                    }
                    let btn_g = ui.selectable_label(*mode == HistogramMode::Green, "G");
                    if btn_g.clicked() {
                        *mode = HistogramMode::Green;
                    }
                    let btn_r = ui.selectable_label(*mode == HistogramMode::Red, "R");
                    if btn_r.clicked() {
                        *mode = HistogramMode::Red;
                    }
                    let btn_rgb = ui.selectable_label(*mode == HistogramMode::Rgb, "RGB");
                    if btn_rgb.clicked() {
                        *mode = HistogramMode::Rgb;
                    }
                });
            });

            ui.add_space(4.0);

            // 圖表畫布區域
            let height = 96.0;
            let width = ui.available_width().max(160.0);
            let (rect, _response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());

            let painter = ui.painter_at(rect);

            // 背景與邊框
            painter.rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(18, 18, 22));
            painter.rect_stroke(
                rect,
                Rounding::same(4.0),
                Stroke::new(1.0_f32, ThemeColors::BORDER),
            );

            // 輔助網格線 (25%, 50%, 75%)
            let grid_color = Color32::from_white_alpha(15);
            for ratio in [0.25, 0.5, 0.75] {
                let x = rect.left() + rect.width() * ratio;
                painter.line_segment(
                    [pos2(x, rect.top()), pos2(x, rect.bottom())],
                    Stroke::new(1.0_f32, grid_color),
                );
                let y = rect.top() + rect.height() * ratio;
                painter.line_segment(
                    [pos2(rect.left(), y), pos2(rect.right(), y)],
                    Stroke::new(1.0_f32, grid_color),
                );
            }

            if let Some(hist) = data {
                if hist.max_count > 0 {
                    let max_val = hist.max_count as f32;

                    // 繪製單一通道折線與填色輔助閉包
                    let draw_channel =
                        |counts: &[u32; 256], stroke_color: Color32, fill_color: Color32| {
                            let bottom_y = rect.bottom() - 2.0;
                            let mut points: Vec<Pos2> = Vec::with_capacity(256);
                            for (i, &count) in counts.iter().enumerate() {
                                let norm_x = i as f32 / 255.0;
                                // 使用平方根非線性縮放，確保亮暗部均勻可視
                                let norm_y = ((count as f32) / max_val).sqrt();
                                let px = rect.left() + norm_x * rect.width();
                                let py = bottom_y - norm_y * (rect.height() - 6.0);
                                points.push(pos2(px, py));
                            }

                            // 構建精確的垂直四邊形條帶 Mesh 填充，徹底杜絕凹多邊形三角剖分穿透問題
                            let mut mesh = egui::Mesh::default();
                            for i in 0..255 {
                                let p_tl = points[i];
                                let p_tr = points[i + 1];
                                let p_bl = pos2(points[i].x, bottom_y);
                                let p_br = pos2(points[i + 1].x, bottom_y);

                                let idx = mesh.vertices.len() as u32;
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos: p_tl,
                                    uv: pos2(0.0, 0.0),
                                    color: fill_color,
                                });
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos: p_tr,
                                    uv: pos2(0.0, 0.0),
                                    color: fill_color,
                                });
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos: p_br,
                                    uv: pos2(0.0, 0.0),
                                    color: fill_color,
                                });
                                mesh.vertices.push(egui::epaint::Vertex {
                                    pos: p_bl,
                                    uv: pos2(0.0, 0.0),
                                    color: fill_color,
                                });

                                mesh.indices.extend_from_slice(&[
                                    idx,
                                    idx + 1,
                                    idx + 2,
                                    idx,
                                    idx + 2,
                                    idx + 3,
                                ]);
                            }
                            painter.add(egui::Shape::mesh(mesh));

                            // 頂部折線輪廓
                            for w in points.windows(2) {
                                painter
                                    .line_segment([w[0], w[1]], Stroke::new(1.2_f32, stroke_color));
                            }
                        };

                    match *mode {
                        HistogramMode::Rgb => {
                            // RGB 疊加：紅、綠、藍各自半透明混合
                            draw_channel(
                                &hist.r,
                                Color32::from_rgb(255, 80, 80),
                                Color32::from_rgba_unmultiplied(255, 40, 40, 35),
                            );
                            draw_channel(
                                &hist.g,
                                Color32::from_rgb(60, 220, 100),
                                Color32::from_rgba_unmultiplied(40, 220, 60, 35),
                            );
                            draw_channel(
                                &hist.b,
                                Color32::from_rgb(70, 160, 255),
                                Color32::from_rgba_unmultiplied(40, 120, 255, 35),
                            );
                        }
                        HistogramMode::Red => {
                            draw_channel(
                                &hist.r,
                                Color32::from_rgb(255, 80, 80),
                                Color32::from_rgba_unmultiplied(255, 40, 40, 60),
                            );
                        }
                        HistogramMode::Green => {
                            draw_channel(
                                &hist.g,
                                Color32::from_rgb(60, 220, 100),
                                Color32::from_rgba_unmultiplied(40, 220, 60, 60),
                            );
                        }
                        HistogramMode::Blue => {
                            draw_channel(
                                &hist.b,
                                Color32::from_rgb(70, 160, 255),
                                Color32::from_rgba_unmultiplied(40, 120, 255, 60),
                            );
                        }
                        HistogramMode::Luma => {
                            draw_channel(
                                &hist.luma,
                                Color32::from_rgb(240, 240, 240),
                                Color32::from_rgba_unmultiplied(220, 220, 220, 50),
                            );
                        }
                    }
                }
            } else {
                // 無資料佔位提示
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "無直方圖資料",
                    egui::FontId::proportional(12.0),
                    ThemeColors::TEXT_MUTED,
                );
            }

            // 底部刻度標記
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("0 (陰影)")
                        .size(10.0)
                        .color(ThemeColors::TEXT_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("255 (高光)")
                            .size(10.0)
                            .color(ThemeColors::TEXT_MUTED),
                    );
                });
            });
        })
        .response
    }
}
