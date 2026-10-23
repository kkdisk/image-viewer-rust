//! 匯出副本不改來源與儲存節點；檔案查詢、編碼及替換在背景執行。
use crate::app::ImageViewerApp;
use core_image::output::{OutputFormat, OutputOptions};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Instant,
};

#[derive(Default)]
pub(crate) struct ExportState {
    open: bool,
    options: OutputOptions,
    path: Option<PathBuf>,
    source: Option<Arc<image::RgbaImage>>,
    identity: Option<(u64, u64)>,
    checking: Option<mpsc::Receiver<bool>>,
    exists: bool,
    worker: Option<mpsc::Receiver<Result<(), String>>>,
    error: Option<String>,
}
impl ExportState {
    pub fn is_busy(&self) -> bool {
        self.worker.is_some()
    }
}
impl ImageViewerApp {
    pub fn begin_export(&mut self) -> bool {
        if self.is_document_busy() || self.is_loading_image() || self.has_pending_leave() {
            return false;
        }
        if self.geometry.is_some()
            || self.is_preview_pending()
            || !self.slider_params.is_identity()
            || self
                .session
                .as_ref()
                .is_some_and(|s| s.has_active_preview())
        {
            self.status_notification =
                Some(("請先套用或取消草稿，再匯出副本".into(), Instant::now()));
            return false;
        }
        let Some(session) = &self.session else {
            return false;
        };
        self.export = ExportState {
            open: true,
            source: Some(session.committed_image()),
            identity: Some(session.identity()),
            ..Default::default()
        };
        true
    }
    pub(crate) fn poll_export(&mut self) {
        if let Some(rx) = &self.export.checking {
            match rx.try_recv() {
                Ok(exists) => {
                    self.export.exists = exists;
                    self.export.checking = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.export.error = Some("無法確認匯出路徑，請重新選擇".into());
                    self.export.path = None;
                    self.export.checking = None;
                }
                _ => {}
            }
        }
        if let Some(rx) = &self.export.worker {
            let result = match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("匯出工作中斷".into())),
                _ => None,
            };
            if let Some(result) = result {
                self.export.worker = None;
                let close_after_export = self.leave_guard.save_then_leave;
                self.leave_guard.save_then_leave = false;
                if close_after_export && result.is_ok() && !self.has_unsaved_changes() {
                    let _ = self.finish_pending_leave();
                }
                match result {
                    Ok(()) => {
                        self.export.open = false;
                        self.export.source = None;
                        self.status_notification =
                            Some(("副本已匯出，來源及編輯儲存狀態不變".into(), Instant::now()));
                    }
                    Err(error) => {
                        self.export.error = Some(format!("匯出失敗，已保留目前工作：{error}"))
                    }
                }
            }
        }
    }
    pub fn export_copy_to(&mut self, path: PathBuf, options: OutputOptions) -> bool {
        if self.is_document_busy()
            || self.is_loading_image()
            || self.geometry.is_some()
            || self.has_pending_leave()
            || self.is_preview_pending()
            || !self.slider_params.is_identity()
        {
            return false;
        }
        let Some(session) = &self.session else {
            return false;
        };
        if session.has_active_preview() {
            return false;
        }
        if !options.matches_path(&path) {
            self.export.error = Some("副檔名與所選格式不一致".into());
            return false;
        }
        if self
            .archive_session
            .as_ref()
            .is_some_and(|archive| archive.is_from_archive(&path))
        {
            self.export.error = Some("壓縮來源必須匯出至外部路徑".into());
            return false;
        }
        let image = session.committed_image();
        let (tx, rx) = mpsc::sync_channel(1);
        self.export.worker = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(
                core_image::output::save_with_options(&image, &path, options)
                    .map_err(|e| e.to_string()),
            );
        });
        true
    }
    pub(crate) fn show_export_dialog(&mut self, ctx: &egui::Context) {
        if !self.export.open {
            return;
        }
        let busy = self.export.is_busy();
        let mut open = true;
        let mut choose = false;
        let mut confirm = false;
        egui::Window::new("匯出副本")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label("匯出不改來源路徑、編輯歷史或已儲存狀態。");
                ui.label("不保留來源 EXIF／GPS／ICC；方向已套用於像素。");
                if self.animated_source {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "動畫來源僅輸出第一格，其他影格不保留。",
                    );
                }
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(
                            &mut self.export.options.format,
                            OutputFormat::Png,
                            "PNG",
                        );
                        ui.selectable_value(
                            &mut self.export.options.format,
                            OutputFormat::Jpeg,
                            "JPEG",
                        );
                    });
                    if self.export.options.format == OutputFormat::Jpeg {
                        ui.add(
                            egui::Slider::new(&mut self.export.options.quality, 1..=100)
                                .text("JPEG 品質"),
                        );
                        ui.horizontal(|ui| {
                            ui.label("透明區域背景");
                            ui.color_edit_button_srgb(&mut self.export.options.background);
                        });
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "JPEG 會重新壓縮，可能降低畫質；透明像素將合成至所選背景。",
                        );
                    } else {
                        ui.label("PNG 保留透明度。");
                    }
                    choose = ui.button("選擇輸出檔案…").clicked();
                    if let Some(path) = &self.export.path {
                        ui.label(path.display().to_string());
                        if self.export.exists {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "目標已存在；確認後將替換此檔案。",
                            );
                        }
                        confirm = ui
                            .add_enabled(
                                self.export.checking.is_none(),
                                egui::Button::new("確認匯出"),
                            )
                            .clicked();
                    }
                });
                if busy || self.export.checking.is_some() {
                    ui.spinner();
                    ctx.request_repaint_after(std::time::Duration::from_millis(16));
                }
                if let Some(error) = &self.export.error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
            });
        if choose {
            let (extension, label) = if self.export.options.format == OutputFormat::Png {
                ("png", "PNG")
            } else {
                ("jpg", "JPEG")
            };
            if let Some(mut path) = rfd::FileDialog::new()
                .add_filter(label, &[extension])
                .set_file_name(format!("匯出副本.{extension}"))
                .save_file()
            {
                if path.extension().is_none() {
                    path.set_extension(extension);
                }
                let (tx, rx) = mpsc::sync_channel(1);
                self.export.path = Some(path.clone());
                self.export.checking = Some(rx);
                self.export.error = None;
                std::thread::spawn(move || {
                    let _ = tx.send(path.exists());
                });
            }
        }
        if confirm && let Some(path) = self.export.path.clone() {
            if self
                .export
                .identity
                .is_some_and(|id| self.session.as_ref().is_none_or(|s| s.identity() != id))
            {
                self.export.error = Some("圖片已變更，請關閉並重新開啟匯出副本".into());
            } else {
                self.export_copy_to(path, self.export.options);
            }
        }
        if !open && !busy {
            self.export = ExportState::default();
        }
    }
}
