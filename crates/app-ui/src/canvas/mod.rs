//! GPU 加速之圖片視口畫布 (平移、滾輪縮放、最適視窗、雙圖分割同步比對視圖)

use std::path::PathBuf;
use egui::{
    pos2, vec2, Color32, FontId, Id, Rect, Response, RichText, Rounding, Sense, Stroke,
    TextureHandle, Ui, UiBuilder,
};
use viewer_state::ViewportState;
use crate::theme::ThemeColors;

/// 雙圖比對來源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComparisonSource {
    #[default]
    Original,  // 原始載入底圖
    Reference, // 外部參考圖片
}

/// 雙圖比對視圖配置
#[derive(Debug, Clone)]
pub struct ComparisonConfig {
    pub is_active: bool,
    pub sync_viewport: bool,
    pub source: ComparisonSource,
    pub reference_path: Option<PathBuf>,
}

impl Default for ComparisonConfig {
    fn default() -> Self {
        Self {
            is_active: false,
            sync_viewport: true,
            source: ComparisonSource::Original,
            reference_path: None,
        }
    }
}

/// 雙圖比對浮動列觸發之動作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonAction {
    ToggleSync,
    SetSource(ComparisonSource),
    OpenReferenceFile,
    CloseComparison,
}

/// 畫布渲染狀態容器
pub struct CanvasState<'a> {
    pub texture: Option<&'a TextureHandle>,
    pub image_dims: Option<(u32, u32)>,
    pub viewport: &'a mut ViewportState,
    pub left_texture: Option<&'a TextureHandle>,
    pub left_dims: Option<(u32, u32)>,
    pub left_viewport: &'a mut ViewportState,
    pub comparison: &'a mut ComparisonConfig,
}

/// 中央影像畫布視圖
pub struct CanvasView;

impl CanvasView {
    /// 繪製中央畫布區域（自動依據 ComparisonConfig 呈現單圖或雙圖比對模式）
    pub fn show(
        ui: &mut Ui,
        state: &mut CanvasState<'_>,
        action: &mut Option<ComparisonAction>,
    ) {
        let available_size = ui.available_size();
        let (rect, _response) = ui.allocate_exact_size(available_size, Sense::hover());

        // 畫布總底色
        ui.painter().rect_filled(rect, 0.0, ThemeColors::BG_CANVAS);

        if !state.comparison.is_active {
            // 單圖模式
            Self::render_viewport(
                ui,
                rect,
                Id::new("single_canvas_viewport"),
                state.texture,
                state.image_dims,
                state.viewport,
                None,
            );
        } else {
            // 雙圖分割模式 (50% : 50%)
            let split_width = (rect.width() - 2.0) * 0.5;
            let left_rect = Rect::from_min_size(rect.min, vec2(split_width, rect.height()));
            let right_rect = Rect::from_min_size(
                pos2(rect.min.x + split_width + 2.0, rect.min.y),
                vec2(split_width, rect.height()),
            );

            // 取得左側標籤名稱
            let left_label = match state.comparison.source {
                ComparisonSource::Original => "📷 原始圖片 (Original)".to_string(),
                ComparisonSource::Reference => {
                    if let Some(p) = &state.comparison.reference_path {
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("參考圖");
                        format!("📑 參考圖: {name}")
                    } else {
                        "📑 外部參考圖 (未載入)".to_string()
                    }
                }
            };

            // 若開啟同步視口，兩者共享右側的主 viewport 狀態
            if state.comparison.sync_viewport {
                // 渲染左側視口
                Self::render_viewport(
                    ui,
                    left_rect,
                    Id::new("left_canvas_viewport"),
                    state.left_texture,
                    state.left_dims,
                    state.viewport,
                    Some(&left_label),
                );

                // 渲染右側視口
                Self::render_viewport(
                    ui,
                    right_rect,
                    Id::new("right_canvas_viewport"),
                    state.texture,
                    state.image_dims,
                    state.viewport,
                    Some("✏️ 目前編輯 (Current Edit)"),
                );
            } else {
                // 獨立視口平移與縮放
                Self::render_viewport(
                    ui,
                    left_rect,
                    Id::new("left_canvas_viewport"),
                    state.left_texture,
                    state.left_dims,
                    state.left_viewport,
                    Some(&left_label),
                );

                Self::render_viewport(
                    ui,
                    right_rect,
                    Id::new("right_canvas_viewport"),
                    state.texture,
                    state.image_dims,
                    state.viewport,
                    Some("✏️ 目前編輯 (Current Edit)"),
                );
            }

            // 繪製中央垂直分割線
            let split_line_x = rect.min.x + split_width + 1.0;
            ui.painter().line_segment(
                [pos2(split_line_x, rect.min.y), pos2(split_line_x, rect.max.y)],
                Stroke::new(2.0, ThemeColors::BORDER),
            );

            // 繪製頂部浮動對比控制條 (Floating HUD)
            Self::render_comparison_hud(ui, rect, state.comparison, action);
        }
    }

    /// 渲染單個視口畫布與處理互動
    fn render_viewport(
        ui: &mut Ui,
        rect: Rect,
        id: Id,
        texture: Option<&TextureHandle>,
        image_dims: Option<(u32, u32)>,
        viewport: &mut ViewportState,
        label: Option<&str>,
    ) -> Response {
        let response = ui.interact(rect, id, Sense::click_and_drag());

        // 設置局部裁切範圍，防止圖片放大平移時超出視口
        let painter = ui.painter().with_clip_rect(rect);

        if let (Some(tex), Some((img_w_u, img_h_u))) = (texture, image_dims) {
            let img_w = img_w_u as f32;
            let img_h = img_h_u as f32;

            // 計算自適應視窗縮放比例 (Auto-fit scale)
            let fit_scale = {
                let margin = 20.0;
                let available_w = (rect.width() - margin).max(10.0);
                let available_h = (rect.height() - margin).max(10.0);
                (available_w / img_w).min(available_h / img_h)
            };

            let current_scale = if viewport.auto_fit {
                viewport.zoom = fit_scale;
                viewport.pan = (0.0, 0.0);
                fit_scale
            } else {
                viewport.zoom
            };

            // 計算圖片位置與繪製尺寸
            let display_size = vec2(img_w * current_scale, img_h * current_scale);
            let center_pos = rect.center() + vec2(viewport.pan.0, viewport.pan.1);
            let image_rect = Rect::from_center_size(center_pos, display_size);

            // 繪製微縮棋盤格襯底
            Self::draw_checkerboard(&painter, image_rect);

            // 繪製 GPU 紋理影像
            painter.image(
                tex.id(),
                image_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );

            // 繪製邊框
            painter.rect_stroke(
                image_rect,
                0.0,
                Stroke::new(1.0, Color32::from_white_alpha(35)),
            );

            // 處理滑鼠拖曳平移
            if response.dragged() {
                let delta = response.drag_delta();
                viewport.pan.0 += delta.x;
                viewport.pan.1 += delta.y;
                viewport.auto_fit = false;
            }

            // 處理滑鼠滾輪縮放 (Zoom at cursor)
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta != 0.0 && response.hovered() {
                let zoom_factor = if scroll_delta > 0.0 { 1.15 } else { 1.0 / 1.15 };
                let cursor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());

                let base_zoom = if viewport.auto_fit {
                    viewport.auto_fit = false;
                    fit_scale
                } else {
                    viewport.zoom
                };

                let new_zoom = (base_zoom * zoom_factor).clamp(0.02, 50.0);
                let actual_factor = new_zoom / base_zoom;

                viewport.pan.0 = cursor.x - rect.center().x - (cursor.x - rect.center().x - viewport.pan.0) * actual_factor;
                viewport.pan.1 = cursor.y - rect.center().y - (cursor.y - rect.center().y - viewport.pan.1) * actual_factor;
                viewport.zoom = new_zoom;
            }

            // 雙擊切換自適應 / 100%
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
                        ui.label(RichText::new("🖼").size(36.0).color(ThemeColors::TEXT_MUTED));
                        ui.add_space(6.0);
                        ui.label(RichText::new("無影像資料").size(14.0).strong().color(ThemeColors::TEXT_PRIMARY));
                    });
                });
            });
        }

        // 繪製角落膠囊標籤 (Pill Badge)
        if let Some(txt) = label {
            let badge_pos = pos2(rect.left() + 12.0, rect.top() + 12.0);
            let font = FontId::proportional(12.0);
            let text_color = ThemeColors::TEXT_PRIMARY;
            let galley = painter.layout_no_wrap(txt.to_string(), font, text_color);
            let badge_rect = Rect::from_min_size(
                badge_pos,
                vec2(galley.size().x + 16.0, galley.size().y + 8.0),
            );

            // 半透明暗色膠囊背景
            painter.rect_filled(badge_rect, Rounding::same(12.0), Color32::from_black_alpha(190));
            painter.rect_stroke(badge_rect, Rounding::same(12.0), Stroke::new(1.0, ThemeColors::BORDER));
            painter.galley(pos2(badge_pos.x + 8.0, badge_pos.y + 4.0), galley, text_color);
        }

        response
    }

    /// 繪製頂部懸浮比對工具列
    fn render_comparison_hud(
        ui: &mut Ui,
        rect: Rect,
        config: &ComparisonConfig,
        action: &mut Option<ComparisonAction>,
    ) {
        let hud_size = vec2(480.0, 36.0);
        let hud_rect = Rect::from_center_size(
            pos2(rect.center().x, rect.top() + 26.0),
            hud_size,
        );

        // 懸浮膠囊框背景
        ui.painter().rect_filled(hud_rect, Rounding::same(18.0), Color32::from_black_alpha(220));
        ui.painter().rect_stroke(hud_rect, Rounding::same(18.0), Stroke::new(1.0, ThemeColors::BORDER));

        ui.allocate_new_ui(UiBuilder::new().max_rect(hud_rect), |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(12.0);

                // 來源切換按鈕
                let is_orig = config.source == ComparisonSource::Original;
                let orig_btn = egui::Button::new(
                    RichText::new("📷 原圖比對")
                        .size(12.0)
                        .color(if is_orig { ThemeColors::ACCENT } else { ThemeColors::TEXT_SECONDARY }),
                );
                if ui.add(orig_btn).clicked() && !is_orig {
                    *action = Some(ComparisonAction::SetSource(ComparisonSource::Original));
                }

                let is_ref = config.source == ComparisonSource::Reference;
                let ref_btn = egui::Button::new(
                    RichText::new("📑 外部參考圖")
                        .size(12.0)
                        .color(if is_ref { ThemeColors::ACCENT } else { ThemeColors::TEXT_SECONDARY }),
                );
                if ui.add(ref_btn).clicked() && !is_ref {
                    *action = Some(ComparisonAction::SetSource(ComparisonSource::Reference));
                }

                if is_ref && ui.button("📁 選擇...").clicked() {
                    *action = Some(ComparisonAction::OpenReferenceFile);
                }

                ui.separator();

                // 同步平移縮放切換
                let sync_text = if config.sync_viewport {
                    RichText::new("🔗 視口已同步").size(12.0).color(ThemeColors::ACCENT)
                } else {
                    RichText::new("⛓ 視口獨立").size(12.0).color(ThemeColors::TEXT_MUTED)
                };
                if ui.button(sync_text).clicked() {
                    *action = Some(ComparisonAction::ToggleSync);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if ui.button("✖ 退出比對").clicked() {
                        *action = Some(ComparisonAction::CloseComparison);
                    }
                });
            });
        });
    }

    /// 繪製微縮棋盤格背景（指示透明區域）
    fn draw_checkerboard(painter: &egui::Painter, rect: Rect) {
        painter.rect_filled(rect, 0.0, Color32::from_rgb(20, 20, 24));

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
                painter.rect_filled(
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
