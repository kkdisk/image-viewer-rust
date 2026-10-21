use egui::{RichText, Ui, UiBuilder};
use crate::theme::ThemeColors;

/// 中央影像畫布視圖
pub struct CanvasView;

impl CanvasView {
    /// 繪製中央畫布區域
    pub fn show(ui: &mut Ui, has_image: bool, image_name: Option<&str>) {
        let (rect, _response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );

        // 畫布底層深暗背景
        ui.painter().rect_filled(rect, 0.0, ThemeColors::BG_CANVAS);

        if !has_image {
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
        } else if let Some(name) = image_name {
            // 暫時提示（Task 2.2 將接入 GPU 紋理畫布）
            ui.allocate_new_ui(UiBuilder::new().max_rect(rect), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        RichText::new(format!("已載入：{name}"))
                            .size(16.0)
                            .color(ThemeColors::TEXT_PRIMARY),
                    );
                });
            });
        }
    }
}
