//! 動畫轉靜態輸出的明確確認；取消及失敗都保留來源與編輯狀態。

use crate::app::ImageViewerApp;
use std::{path::PathBuf, sync::Arc};

pub(crate) struct AnimationSave {
    pub path: PathBuf,
    pub source: Arc<image::RgbaImage>,
}

impl ImageViewerApp {
    pub fn is_animated_source(&self) -> bool {
        self.animated_source
    }

    pub fn has_animation_save_confirmation(&self) -> bool {
        self.animation_save.is_some()
    }

    pub fn resolve_animation_save(&mut self, confirm: bool) -> bool {
        let Some(request) = self.animation_save.take() else {
            return false;
        };
        if !confirm
            || !self.session.as_ref().is_some_and(|session| {
                Arc::ptr_eq(&session.committed_image(), &request.source)
                    && !session.has_active_preview()
            })
            || !self.slider_params.is_identity()
        {
            self.leave_guard.save_then_leave = false;
            return false;
        }
        if !self.enqueue_save(request.path) {
            self.leave_guard.save_then_leave = false;
            return false;
        }
        true
    }

    pub(crate) fn show_animation_save_dialog(&mut self, ctx: &egui::Context) {
        let mut choice = None;
        let mut open = true;
        egui::Window::new("動畫檔儲存警告")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("此操作會變成靜態圖，僅保留目前畫面的第一格編輯結果。");
                ui.label("原有動畫影格將不會保留於輸出檔案。");
                if let Some(request) = &self.animation_save {
                    ui.label(request.path.display().to_string());
                }
                ui.horizontal(|ui| {
                    if ui.button("確認儲存靜態圖").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("取消").clicked() {
                        choice = Some(false);
                    }
                });
            });
        if !open || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            choice = Some(false);
        }
        if let Some(confirm) = choice {
            self.resolve_animation_save(confirm);
        }
    }
}
