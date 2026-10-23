//! 看圖便利操作；磁碟與像素工作只在背景執行。
use crate::app::ImageViewerApp;
use std::{
    path::Path,
    sync::{Arc, mpsc},
    time::SystemTime,
};

#[derive(Default)]
pub struct FileInfo {
    pub photo: core_image::exif::PhotoMetadata,
    pub full_path: Option<std::path::PathBuf>,
    pub bytes: Option<u64>,
    pub modified: Option<SystemTime>,
    pub format: String,
}

impl FileInfo {
    pub(crate) fn read(path: &Path) -> Self {
        let metadata = std::fs::metadata(path).ok();
        Self {
            photo: core_image::exif::read_photo_metadata(path),
            full_path: std::path::absolute(path).ok(),
            bytes: metadata.as_ref().map(|m| m.len()),
            modified: metadata.and_then(|m| m.modified().ok()),
            format: image::ImageReader::open(path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.format())
                .map_or_else(|| "未知".into(), |f| format!("{f:?}")),
        }
    }
}

pub use core_image::geometry::Transform;
struct GeometryResult {
    source: Arc<image::RgbaImage>,
    pixels: image::RgbaImage,
    transform: Transform,
}
#[derive(Default)]
pub struct ConvenienceState {
    pub info_open: bool,
    pub info_tab: bool,
    pub info: FileInfo,
    geometry: Option<mpsc::Receiver<GeometryResult>>,
}
impl ImageViewerApp {
    pub fn is_transforming(&self) -> bool {
        self.convenience.geometry.is_some()
    }
    pub fn transform_image(&mut self, transform: Transform) -> bool {
        if self.is_document_busy()
            || self.is_loading_image()
            || self.has_pending_leave()
            || self.is_transforming()
            || !self.slider_params.is_identity()
            || self.is_preview_pending()
            || self.debouncer.is_computing()
            || self.session.as_ref().is_none_or(|s| s.has_active_preview())
        {
            return false;
        }
        let Some(source) = self.session.as_ref().map(|s| s.committed_image()) else {
            return false;
        };
        let (tx, rx) = mpsc::sync_channel(1);
        self.convenience.geometry = Some(rx);
        std::thread::spawn(move || {
            let pixels = transform.apply(&source);
            let _ = tx.send(GeometryResult {
                source,
                pixels,
                transform,
            });
        });
        true
    }
    pub fn poll_convenience(&mut self) {
        let result = self.convenience.geometry.as_ref().map(|rx| rx.try_recv());
        match result {
            Some(Ok(result)) => {
                self.convenience.geometry = None;
                if let Some(session) = &mut self.session
                    && Arc::ptr_eq(&session.committed_image(), &result.source)
                    && self.slider_params.is_identity()
                    && !session.has_active_preview()
                {
                    session.push_action(result.pixels, result.transform.label());
                    self.viewport.reset_fit_window();
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.convenience.geometry = None;
            }
            _ => {}
        }
    }
    pub fn display_source_path(&self) -> Option<String> {
        let path = self.session.as_ref()?.source_path()?;
        if let Some(archive) = &self.archive_session
            && archive.is_from_archive(path)
        {
            let root = archive.temp_dir.path();
            // 顯示來源識別，不把受管理的暫存位置交給使用者。
            return Some(format!(
                "{} :: {}",
                archive.archive_path.display(),
                path.strip_prefix(root).unwrap_or(path).display()
            ));
        }
        Some(
            self.convenience
                .info
                .full_path
                .as_deref()
                .unwrap_or(path)
                .display()
                .to_string(),
        )
    }
    pub fn copy_source_path(&self, ctx: &egui::Context) {
        if let Some(path) = self.display_source_path() {
            ctx.copy_text(path);
        }
    }
    pub(crate) fn history_details(&self) -> Option<String> {
        let session = self.session.as_ref()?;
        let h = session.history();
        Some(format!(
            "可復原 {} 步／重做 {} 步；影像歷史 {:.1}／{:.0} MiB{}\n未套用預覽 {:.1} MiB；背景中間值、匯出快照與 GPU 未計入，並非程序硬上限。",
            h.undo_len(),
            h.redo_len(),
            h.retained_bytes() as f64 / 1048576.0,
            h.budget() as f64 / 1048576.0,
            if h.retained_bytes() > h.budget() {
                "（必要影像超出軟預算）"
            } else {
                "（最多 20 步，受預算限制）"
            },
            session.preview_bytes() as f64 / 1048576.0,
        ))
    }

    pub(crate) fn show_inspector(
        &mut self,
        ctx: &egui::Context,
        action: &mut Option<crate::panels::AdjustmentAction>,
    ) {
        if !self.show_side_panel {
            return;
        }
        egui::SidePanel::right("side_adjustment_panel")
            .resizable(true)
            .default_width(320.0)
            .width_range(280.0..=440.0)
            .frame(
                egui::Frame::none()
                    .fill(crate::theme::ThemeColors::BG_PANEL)
                    .inner_margin(egui::Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.convenience.info_tab, false, "調整");
                    ui.selectable_value(&mut self.convenience.info_tab, true, "資訊");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("×").on_hover_text("收合側欄").clicked() {
                            self.show_side_panel = false;
                        }
                    });
                });
                ui.separator();
                if self.convenience.info_tab {
                    if ui.button("另開資訊浮窗").clicked() {
                        self.convenience.info_open = true;
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("image_info_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| self.image_info_contents(ui));
                } else {
                    let has_image = self.session.is_some() && self.geometry.is_none();
                    let is_computing = self.is_preview_pending();
                    crate::panels::AdjustmentPanel::contents(
                        ui,
                        &mut crate::panels::AdjustmentPanelState {
                            white_balance_open: &mut self.white_balance_open,
                            fine_tune_open: &mut self.fine_tune_open,
                            is_open: &mut self.show_side_panel,
                            has_image,
                            params: &mut self.slider_params,
                            is_computing,
                            histogram_data: self.cached_histogram.as_ref(),
                            histogram_mode: &mut self.histogram_mode,
                        },
                        action,
                    );
                }
            });
    }

    pub(crate) fn show_image_info(&mut self, ctx: &egui::Context) {
        let mut open = self.convenience.info_open;
        egui::Window::new("圖片資訊（唯讀）")
            .open(&mut open)
            .default_width(320.0)
            .frame(egui::Frame::window(&ctx.style()).fill(crate::theme::ThemeColors::BG_PANEL))
            .vscroll(true)
            .show(ctx, |ui| self.image_info_contents(ui));
        self.convenience.info_open = open;
    }

    fn image_info_contents(&self, ui: &mut egui::Ui) {
        let Some(session) = &self.session else {
            ui.label("未載入圖片");
            return;
        };
        let photo = &self.convenience.info.photo;
        let info = &self.convenience.info;
        let dimensions = session.active_image().dimensions();
        ui.heading("圖片資訊");
        ui.label(
            egui::RichText::new("唯讀資訊")
                .small()
                .color(crate::theme::ThemeColors::TEXT_SECONDARY),
        );
        ui.add_space(6.0);
        ui.label(format!(
            "{}：{}",
            photo.datetime_source.unwrap_or("拍攝時間"),
            photo
                .captured_at
                .as_deref()
                .unwrap_or("未記錄有效 EXIF 日期時間")
        ));
        ui.label(format!("尺寸：{} × {} px", dimensions.0, dimensions.1));
        ui.label(format!("格式：{}", info.format));
        ui.label(format!(
            "檔案大小：{}",
            info.bytes.map_or_else(
                || "未知".into(),
                |n| format!("{:.2} MiB", n as f64 / 1048576.0)
            )
        ));
        ui.separator();
        ui.label("來源路徑");
        let path = self.display_source_path();
        let path = path.as_deref().unwrap_or("未命名");
        ui.add(egui::Label::new(path).truncate())
            .on_hover_text(path);
        if ui
            .add_enabled(
                self.session
                    .as_ref()
                    .and_then(|s| s.source_path())
                    .is_some(),
                egui::Button::new("複製完整路徑"),
            )
            .clicked()
        {
            self.copy_source_path(ui.ctx());
        }
        ui.separator();
        egui::CollapsingHeader::new("進階資訊").show(ui, |ui| {
            ui.label(format!(
                "EXIF 時區：{}",
                photo
                    .timezone
                    .as_deref()
                    .unwrap_or("未記錄，保留相機當地時間")
            ));
            ui.label(format!("來源 EXIF 方向：{}", photo.orientation_label()));
            ui.label(format!(
                "原始尺寸：{} × {} px",
                session.original_image().width(),
                session.original_image().height()
            ));
            ui.label(format!("修改時間：{}", format_timestamp(info.modified)));
            ui.label(format!(
                "檔案大小：{}",
                info.bytes
                    .map_or_else(|| "未知".into(), |n| format!("{n} 位元組"))
            ));
            ui.label(if self.animated_source {
                "動畫：僅顯示第一格"
            } else {
                "靜態圖片"
            });
            if self.is_current_image_from_archive() {
                ui.label("壓縮檔來源：只能另存外部");
            }
        });
        if let Some(history) = self.history_details() {
            egui::CollapsingHeader::new("編輯歷史與記憶體").show(ui, |ui| {
                ui.label(history);
            });
        }
    }
}

fn format_timestamp(time: Option<SystemTime>) -> String {
    let Some(seconds) = time
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
    else {
        return "未知".into();
    };
    // 以四百年閏年週期換算公曆日期，時間明確標為 UTC。
    let days = seconds / 86400 + 719468;
    let era = days / 146097;
    let day_of_era = days % 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month + 2) / 5 + 1;
    let (year, month) = if month < 10 {
        (year, month + 3)
    } else {
        (year + 1, month - 9)
    };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_info_clipboard_and_utc_dates_are_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("來源圖片.png");
        image::RgbaImage::new(4, 3).save(&path).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut app = ImageViewerApp::default();
        app.open_image_path(path.clone()).unwrap();
        assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
        assert_eq!(app.convenience.info.format, "Png");
        assert_eq!(app.convenience.info.bytes, Some(before.len() as u64));
        let ctx = egui::Context::default();
        app.convenience.info_open = true;
        let output = ctx.run(Default::default(), |ctx| {
            app.show_image_info(ctx);
            app.copy_source_path(ctx);
        });
        assert_eq!(
            output.platform_output.copied_text,
            path.display().to_string()
        );
        assert!(!app.has_unsaved_changes());
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert_eq!(
            format_timestamp(Some(SystemTime::UNIX_EPOCH)),
            "1970-01-01 00:00:00 UTC"
        );
        assert_eq!(
            format_timestamp(Some(
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(951782400)
            )),
            "2000-02-29 00:00:00 UTC"
        );
    }
    #[test]
    fn archive_paths_keep_nested_source_and_disable_file_deletion() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("圖片集.zip");
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&archive_path).unwrap());
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(3, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        writer
            .start_file("子目錄/圖片.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes.get_ref()).unwrap();
        writer.finish().unwrap();
        let mut app = ImageViewerApp::default();
        app.open_file_path(archive_path.clone()).unwrap();
        assert!(app.wait_archive_loaded(std::time::Duration::from_secs(5)));
        let text = app.display_source_path().unwrap();
        assert!(text.contains("圖片集.zip :: 子目錄"));
        assert!(
            !text.contains(
                &app.archive_session
                    .as_ref()
                    .unwrap()
                    .temp_dir
                    .path()
                    .display()
                    .to_string()
            )
        );
        assert!(!app.request_delete_current());
        app.refresh_directory(true);
        assert!(!app.is_scanning_directory());
        assert!(archive_path.exists());
    }
    fn finish(app: &mut ImageViewerApp) {
        let start = std::time::Instant::now();
        while app.is_transforming() {
            app.poll_convenience();
            assert!(start.elapsed().as_secs() < 5);
            std::thread::yield_now();
        }
    }
    #[test]
    fn geometry_preserves_pixels_alpha_and_history() {
        let pixels = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([x as u8, y as u8, 0, (x + y) as u8])
        });
        for transform in [
            Transform::Clockwise,
            Transform::Counterclockwise,
            Transform::Horizontal,
            Transform::Vertical,
        ] {
            let mut app = ImageViewerApp::default();
            app.session = Some(viewer_state::EditingSession::new(pixels.clone(), None));
            assert!(app.transform_image(transform));
            assert!(!app.transform_image(transform));
            finish(&mut app);
            let expected = transform.apply(&pixels);
            assert_eq!(*app.session.as_ref().unwrap().committed_image(), expected);
            assert!(app.has_unsaved_changes());
            app.undo();
            assert_eq!(*app.session.as_ref().unwrap().committed_image(), pixels);
            assert!(!app.has_unsaved_changes());
            app.redo();
            assert_eq!(*app.session.as_ref().unwrap().committed_image(), expected);
        }
    }
    #[test]
    fn pending_preview_and_stale_geometry_do_not_commit() {
        let mut app = ImageViewerApp::default();
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(3, 2),
            None,
        ));
        app.slider_params.brightness = 1.2;
        assert!(!app.transform_image(Transform::Clockwise));
        app.slider_params = Default::default();
        assert!(app.transform_image(Transform::Clockwise));
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(8, 9),
            None,
        ));
        finish(&mut app);
        assert_eq!(
            app.session.as_ref().unwrap().committed_image().dimensions(),
            (8, 9)
        );
        assert!(!app.has_unsaved_changes());
    }
    #[test]
    fn jpeg_requires_confirmation_and_cancel_preserves_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("來源.jpg");
        let pixels = image::RgbaImage::new(3, 2);
        core_image::save_image_atomic(&image::DynamicImage::ImageRgba8(pixels.clone()), &path)
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut app = ImageViewerApp::default();
        app.session = Some(viewer_state::EditingSession::new(
            pixels,
            Some(path.clone()),
        ));
        assert!(app.save_current_image());
        assert!(app.has_animation_save_confirmation());
        assert!(!app.resolve_animation_save(false));
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    fn text_position(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| text_position(s, label)),
            _ => None,
        }
    }

    #[test]
    fn inspector_tabs_preserve_preview_canvas_and_visible_footer() {
        for pixels_per_point in [1.0, 2.0] {
            for strip_height in [110.0, 220.0] {
                let ctx = egui::Context::default();
                crate::theme::apply_theme(&ctx);
                ctx.set_pixels_per_point(pixels_per_point);
                let mut app = ImageViewerApp::default();
                app.session = Some(viewer_state::EditingSession::new(
                    image::RgbaImage::new(4, 3),
                    None,
                ));
                app.slider_params.brightness = 1.2;
                let frame = |app: &mut ImageViewerApp, events| {
                    let mut canvas = egui::Rect::NOTHING;
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(800.0, 600.0),
                            )),
                            events,
                            system_theme: Some(egui::Theme::Light),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::TopBottomPanel::top("test_menu")
                                .exact_height(30.0)
                                .show(ctx, |_| {});
                            egui::TopBottomPanel::bottom("test_footer")
                                .exact_height(64.0 + strip_height)
                                .show(ctx, |_| {});
                            let mut action = None;
                            app.show_inspector(ctx, &mut action);
                            assert_eq!(action, None);
                            egui::CentralPanel::default().show(ctx, |ui| {
                                canvas = ui.max_rect();
                            });
                        },
                    );
                    (output, canvas)
                };
                for _ in 0..4 {
                    frame(&mut app, vec![]);
                }
                let (output, baseline) = frame(&mut app, vec![]);
                let info = output
                    .shapes
                    .iter()
                    .find_map(|s| text_position(&s.shape, "資訊"))
                    .unwrap();
                for label in ["重設", "取消", "套用"] {
                    let pos = output
                        .shapes
                        .iter()
                        .find_map(|s| text_position(&s.shape, label))
                        .unwrap();
                    assert!(
                        pos.y > 30.0 && pos.y < 600.0 - 64.0 - strip_height,
                        "按鈕被遮蔽：{label} {pos:?}"
                    );
                }
                for pressed in [true, false] {
                    frame(
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(info),
                            egui::Event::PointerButton {
                                pos: info,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
                assert!(app.convenience.info_tab);
                assert_eq!(frame(&mut app, vec![]).1, baseline, "切換分頁不可改變畫布");
                let (output, _) = frame(&mut app, vec![]);
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|s| text_position(&s.shape, "圖片資訊").is_some())
                );
                assert!(
                    ctx.style().visuals.dark_mode,
                    "系統淺色模式不能覆蓋深色介面"
                );
                app.convenience.info_tab = false;
                let (_, canvas) = frame(&mut app, vec![]);

                assert_eq!(canvas, baseline);
                assert!(canvas.width() > 400.0);
                assert_eq!(app.slider_params.brightness, 1.2);
                assert!(!app.session.as_ref().unwrap().is_dirty());
                assert!(!app.session.as_ref().unwrap().can_undo());
            }
        }
    }
}
