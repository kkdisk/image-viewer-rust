//! 離開編輯工作前的確認狀態，涵蓋切圖、壓縮檔替換與關閉。

use crate::app::ImageViewerApp;
use core_archive::ManagedArchiveSession;
use std::path::PathBuf;

pub(crate) enum LeaveAction {
    Image(PathBuf),
    ArchivePath(PathBuf),
    Archive(ManagedArchiveSession),
    Close,
    Loaded(Box<crate::document_io::LoadedImage>),
}

#[derive(Default)]
pub(crate) struct LeaveGuard {
    pub action: Option<LeaveAction>,
    pub error: Option<String>,
    pub close_confirmed: bool,
    pub save_then_leave: bool,
}

#[derive(Clone, Copy)]
pub enum LeaveChoice {
    Save,
    Discard,
    Cancel,
}

impl ImageViewerApp {
    pub(crate) fn show_guarded_editor(&mut self, ctx: &egui::Context) {
        // 確認期間凍結編輯控制，僅保留目前圖片與對話框，避免底層操作繞過保護。
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(crate::theme::ThemeColors::BG_CANVAS))
            .show(ctx, |ui| {
                if let Some(texture) = &self.texture {
                    ui.centered_and_justified(|ui| {
                        ui.add(
                            egui::Image::new(texture)
                                .max_size(ui.available_size())
                                .maintain_aspect_ratio(true),
                        );
                    });
                }
            });
        if self.animation_save.is_some() {
            self.show_animation_save_dialog(ctx);
            return;
        }
        self.show_leave_dialog(ctx);
        if self.is_saving() {
            egui::Window::new("正在儲存圖片")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 110.0))
                .show(ctx, |ui| {
                    ui.spinner();
                    ui.label("完成原子寫入後才繼續操作，請稍候…");
                });
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub fn has_pending_leave(&self) -> bool {
        self.leave_guard.action.is_some()
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.is_dirty())
            || (self.session.is_some() && !self.slider_params.is_identity())
    }

    pub(crate) fn defer_leave(&mut self, action: LeaveAction) -> bool {
        if self.has_pending_leave() {
            return true;
        }
        if !self.has_unsaved_changes() {
            return false;
        }
        self.cancel_archive_loading();
        self.leave_guard.action = Some(action);
        self.leave_guard.error = None;
        true
    }

    pub fn request_close(&mut self) -> bool {
        if self.leave_guard.close_confirmed {
            return true;
        }
        if self.is_saving() {
            if self.leave_guard.action.is_none() {
                self.leave_guard.action = Some(LeaveAction::Close);
            }
            self.leave_guard.save_then_leave = true;
            return false;
        }
        !self.defer_leave(LeaveAction::Close)
    }

    /// 預覽尚未套用時拒絕儲存並離開，避免把底圖誤當成已儲存的預覽。
    pub fn resolve_pending_leave(&mut self, choice: LeaveChoice) -> Result<bool, String> {
        if !self.has_pending_leave() {
            return Ok(false);
        }
        if matches!(choice, LeaveChoice::Cancel) {
            self.leave_guard.action = None;
            self.leave_guard.error = None;
            self.leave_guard.save_then_leave = false;
            return Ok(false);
        }
        if self.is_saving() {
            return Err("儲存進行中，請稍候或取消離開".into());
        }
        if matches!(choice, LeaveChoice::Save) {
            if self
                .session
                .as_ref()
                .is_some_and(|s| s.has_active_preview())
                || !self.slider_params.is_identity()
            {
                let error = "預覽尚未套用。請取消離開，先按「套用」後再儲存。".to_owned();
                self.leave_guard.error = Some(error.clone());
                return Err(error);
            }
            if !self.save_current_image() {
                let error = "尚未完成儲存，已保留目前工作。可重試、放棄變更或取消離開。".to_owned();
                self.leave_guard.error = Some(error.clone());
                return Err(error);
            }
            self.leave_guard.save_then_leave = true;
            return Ok(false);
        }
        self.finish_pending_leave()
    }

    pub(crate) fn finish_pending_leave(&mut self) -> Result<bool, String> {
        let Some(action) = self.leave_guard.action.take() else {
            return Ok(false);
        };
        self.leave_guard.error = None;
        let result = match action {
            LeaveAction::ArchivePath(path) => {
                self.start_archive_path(path);
                Ok(())
            }
            LeaveAction::Image(path) => self
                .open_image_path_unchecked(path)
                .map_err(|e| e.to_string()),
            LeaveAction::Archive(session) => self.apply_archive_session_unchecked(session),
            LeaveAction::Loaded(loaded) => {
                self.install_loaded_image(*loaded);
                Ok(())
            }
            LeaveAction::Close => {
                self.leave_guard.close_confirmed = true;
                Ok(())
            }
        };
        if let Err(error) = &result {
            self.status_notification = Some((error.clone(), std::time::Instant::now()));
        }
        result.map(|()| true)
    }

    pub(crate) fn show_leave_dialog(&mut self, ctx: &egui::Context) {
        if !self.has_pending_leave() {
            return;
        }
        egui::Area::new(egui::Id::new("unsaved_backdrop"))
            .order(egui::Order::Foreground)
            .interactable(false)
            .fixed_pos(ctx.screen_rect().min)
            .show(ctx, |ui| {
                let rect = ctx.screen_rect();
                ui.allocate_rect(rect, egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(120));
            });
        let mut choice = None;
        let mut open = true;
        egui::Window::new("尚有未儲存的變更")
            .id(egui::Id::new("unsaved_changes_dialog"))
            .order(egui::Order::Foreground)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("儲存目前變更後繼續，或放棄變更？");
                if self
                    .session
                    .as_ref()
                    .is_some_and(|s| s.has_active_preview())
                    || !self.slider_params.is_identity()
                {
                    ui.label("預覽尚未套用：請選「取消」回到編輯，先按「套用」再儲存。");
                }
                if let Some(error) = &self.leave_guard.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("儲存並繼續").clicked() {
                        choice = Some(LeaveChoice::Save);
                    }
                    if ui.button("放棄變更並繼續").clicked() {
                        choice = Some(LeaveChoice::Discard);
                    }
                    if ui.button("取消").clicked() {
                        choice = Some(LeaveChoice::Cancel);
                    }
                });
            });
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            choice = Some(LeaveChoice::Cancel);
        }
        if let Some(choice) = choice {
            let _ = self.resolve_pending_leave(choice);
            ctx.request_repaint();
        }
        if self.leave_guard.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, RawInput};

    fn frame(
        app: &mut ImageViewerApp,
        ctx: &egui::Context,
        events: Vec<Event>,
    ) -> egui::FullOutput {
        ctx.run(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.show_guarded_editor(ctx),
        )
    }

    fn text_position(shape: &egui::Shape, label: &str) -> Option<Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_position(shape, label)),
            _ => None,
        }
    }

    fn click(app: &mut ImageViewerApp, ctx: &egui::Context, label: &str) -> egui::FullOutput {
        frame(app, ctx, Vec::new());
        let output = frame(app, ctx, Vec::new());
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| text_position(&shape.shape, label))
            .expect("找不到確認按鈕");
        frame(
            app,
            ctx,
            vec![
                Event::PointerMoved(pos),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(
            app,
            ctx,
            vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        )
    }

    #[test]
    fn animation_dialog_cancel_and_confirm_buttons_gate_writes() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("static.png");
        let mut app = ImageViewerApp::default();
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(4, 4),
            None,
        ));
        app.animated_source = true;
        assert!(app.save_image_to(&target));
        frame(&mut app, &ctx, Vec::new());
        let output = frame(&mut app, &ctx, Vec::new());
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| text_position(&shape.shape, "動畫檔儲存警告").is_some())
        );
        click(&mut app, &ctx, "取消");
        assert!(!app.has_animation_save_confirmation());
        assert!(!target.exists());
        assert!(app.save_image_to(&target));
        click(&mut app, &ctx, "確認儲存靜態圖");
        assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
        assert!(target.exists());
        assert!(!app.is_animated_source());
    }

    #[test]
    fn dialog_cancel_and_discard_buttons_handle_real_pointer_events() {
        let ctx = egui::Context::default();
        let mut app = ImageViewerApp::default();
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(4, 4),
            None,
        ));
        app.session
            .as_mut()
            .unwrap()
            .push_action(image::RgbaImage::new(4, 4), "測試修改");
        assert!(!app.request_close());
        click(&mut app, &ctx, "取消");
        assert!(!app.has_pending_leave());
        assert!(app.has_unsaved_changes());
        assert!(!app.request_close());
        let output = click(&mut app, &ctx, "放棄變更並繼續");
        assert!(!app.has_pending_leave());
        assert!(app.request_close());
        assert!(
            output
                .viewport_output
                .values()
                .any(|viewport| viewport.commands.contains(&egui::ViewportCommand::Close))
        );
    }
}
