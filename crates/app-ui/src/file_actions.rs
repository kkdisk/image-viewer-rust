//! 背景重掃與回收；回收失敗絕不改成永久刪除。
use crate::{app::ImageViewerApp, playlist::ImagePlaylist};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Instant,
};

struct ScanResult {
    source: Arc<image::RgbaImage>,
    path: PathBuf,
    reload: bool,
    params: core_image::AdjustmentParams,
    result: Result<ImagePlaylist, String>,
    info: crate::convenience::FileInfo,
}

#[derive(Default)]
pub struct FileActions {
    scan: Option<mpsc::Receiver<ScanResult>>,
    recycle: Option<mpsc::Receiver<Result<(), String>>>,
    pub confirming_delete: bool,
    pub skip_confirmation: bool,
    focused: bool,
    rescan_reload: bool,
}
impl ImageViewerApp {
    pub fn is_recycling(&self) -> bool {
        self.file_actions.recycle.is_some()
    }
    pub fn is_scanning_directory(&self) -> bool {
        self.file_actions.scan.is_some()
    }
    pub fn refresh_directory(&mut self, reload: bool) {
        self.stop_slideshow();
        if self.is_document_busy()
            || self.is_loading_image()
            || self.has_pending_leave()
            || self.file_actions.confirming_delete
            || self.is_current_image_from_archive()
        {
            return;
        }
        if reload && self.defer_leave(crate::navigation_guard::LeaveAction::Refresh) {
            return;
        }
        self.start_refresh(reload);
    }
    pub(crate) fn start_refresh(&mut self, reload: bool) {
        if self.file_actions.scan.is_some() {
            self.file_actions.rescan_reload |= reload;
            return;
        }
        let Some(session) = &self.session else {
            return;
        };
        let Some(path) = session.source_path().map(PathBuf::from) else {
            return;
        };
        let Some(dir) = path.parent().map(PathBuf::from) else {
            return;
        };
        let source = session.active_image();
        let params = self.slider_params;
        let sort = self.preferences.current.sort;
        let (tx, rx) = mpsc::sync_channel(1);
        self.file_actions.scan = Some(rx);
        std::thread::spawn(move || {
            let mut list = ImagePlaylist::new();
            list.set_sort(sort);
            let result = list
                .scan_directory(&dir)
                .map(|()| list)
                .map_err(|e| e.to_string());
            let info = crate::convenience::FileInfo::read(&path);
            let _ = tx.send(ScanResult {
                source,
                path,
                reload,
                params,
                result,
                info,
            });
        });
    }
    pub(crate) fn poll_file_actions(&mut self) {
        if let Some(result) = self.file_actions.scan.as_ref().map(|rx| rx.try_recv()) {
            match result {
                Ok(scan) => {
                    self.file_actions.scan = None;
                    let queued_reload = std::mem::take(&mut self.file_actions.rescan_reload);
                    let reload = scan.reload || queued_reload;
                    if !self.session.as_ref().is_some_and(|s| {
                        s.source_path() == Some(scan.path.as_path())
                            && Arc::ptr_eq(&s.active_image(), &scan.source)
                    }) {
                        return;
                    }
                    match scan.result {
                        Ok(mut list) => {
                            let exists = list.try_select_existing(&scan.path);
                            list.set_sort(self.preferences.current.sort);
                            self.playlist = list;
                            self.thumbnail_manager.clear();
                            self.convenience.info = scan.info;
                            if reload && exists && self.slider_params == scan.params {
                                let _ = self.open_image_path_unchecked(scan.path);
                            } else if !exists {
                                self.status_notification = Some((
                                    "目前圖片已不在資料夾內；保留目前工作，可另存或選擇其他圖片"
                                        .into(),
                                    Instant::now(),
                                ));
                            }
                        }
                        Err(error) => {
                            self.status_notification =
                                Some((format!("重新整理失敗：{error}"), Instant::now()))
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.file_actions.scan = None;
                    self.file_actions.rescan_reload = false;
                }
                _ => {}
            }
        }
        if let Some(result) = self.file_actions.recycle.as_ref().map(|rx| rx.try_recv()) {
            match result {
                Ok(result) => {
                    self.file_actions.recycle = None;
                    self.finish_recycle(result);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.file_actions.recycle = None;
                    self.status_notification = Some((
                        "回收工作中斷，保留目前工作，請重新整理確認來源".into(),
                        Instant::now(),
                    ));
                }
                _ => {}
            }
        }
    }
    pub fn request_delete_current(&mut self) -> bool {
        self.stop_slideshow();
        if self.is_document_busy()
            || self.file_actions.confirming_delete
            || self.has_pending_leave()
            || self.is_loading_image()
            || self.is_current_image_from_archive()
            || self
                .session
                .as_ref()
                .is_none_or(|s| s.source_path().is_none())
        {
            return false;
        }
        if self.defer_leave(crate::navigation_guard::LeaveAction::Delete) {
            return true;
        }
        self.begin_delete_confirmation();
        true
    }
    pub(crate) fn begin_delete_confirmation(&mut self) {
        if self.preferences.current.confirm_delete {
            self.file_actions.confirming_delete = true;
        } else {
            self.start_recycle();
        }
    }
    pub fn resolve_delete_current(&mut self, confirm: bool) {
        if !self.file_actions.confirming_delete {
            return;
        }
        self.file_actions.confirming_delete = false;
        if confirm {
            if self.file_actions.skip_confirmation {
                self.preferences.current.confirm_delete = false;
            }
            self.start_recycle();
        }
        self.file_actions.skip_confirmation = false;
    }
    fn start_recycle(&mut self) {
        if self.is_current_image_from_archive() || self.is_document_busy() {
            return;
        }
        let Some(path) = self
            .session
            .as_ref()
            .and_then(|s| s.source_path())
            .map(PathBuf::from)
        else {
            return;
        };
        self.file_actions.scan = None;
        self.file_actions.rescan_reload = false;
        let (tx, rx) = mpsc::sync_channel(1);
        self.file_actions.recycle = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(core_archive::recycle::recycle_file(&path).map_err(|e| e.to_string()));
        });
    }
    fn finish_recycle(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let Some(path) = self
                    .session
                    .as_ref()
                    .and_then(|s| s.source_path())
                    .map(PathBuf::from)
                else {
                    return;
                };
                self.remove_recent(path.clone());
                let next = self.playlist.remove_file(&path);
                self.session = None;
                self.debouncer.cancel_pending();
                self.slider_params = Default::default();
                self.texture = None;
                self.original_texture = None;
                self.reference_texture = None;
                self.reference_image = None;
                self.comparison.is_active = false;
                self.thumbnail_manager.clear();
                if let Some(path) = next {
                    let _ = self.open_image_path_unchecked(path);
                }
                self.status_notification =
                    Some(("圖片已移至回收桶，可由作業系統還原".into(), Instant::now()));
            }
            Err(error) => {
                self.status_notification = Some((
                    format!("無法移至回收桶，未執行永久刪除：{error}"),
                    Instant::now(),
                ))
            }
        }
    }
    pub(crate) fn handle_focus_refresh(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.focused);
        let gained = focused && !self.file_actions.focused;
        self.file_actions.focused = focused;
        if gained && !self.slideshow.playing {
            self.refresh_directory(false);
        }
        if self.is_scanning_directory() || self.is_recycling() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    pub(crate) fn show_delete_dialog(&mut self, ctx: &egui::Context) {
        let mut open = true;
        let mut confirm = None;
        egui::Window::new("移至回收桶？")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(self.display_source_path().unwrap_or_default());
                ui.label("檔案刪除不屬於編輯復原，請至作業系統回收桶還原。");
                ui.checkbox(
                    &mut self.file_actions.skip_confirmation,
                    "不再詢問（可由選單還原）",
                );
                ui.horizontal(|ui| {
                    if ui.button("移至回收桶").clicked() {
                        confirm = Some(true);
                    }
                    if ui.button("取消").clicked() {
                        confirm = Some(false);
                    }
                });
            });
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            confirm = Some(false);
        }
        if let Some(confirm) = confirm {
            self.resolve_delete_current(confirm);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::navigation_guard::LeaveChoice;
    use std::time::Duration;
    fn setup() -> (tempfile::TempDir, ImageViewerApp, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("圖片1.png");
        image::RgbaImage::new(3, 2).save(&path).unwrap();
        image::RgbaImage::new(2, 4)
            .save(dir.path().join("圖片2.png"))
            .unwrap();
        let mut app = ImageViewerApp::default();
        app.open_image_path(path.clone()).unwrap();
        assert!(app.wait_document_io(Duration::from_secs(5)));
        (dir, app, path)
    }
    fn finish_scan(app: &mut ImageViewerApp) {
        let start = Instant::now();
        while app.is_scanning_directory() {
            app.poll_file_actions();
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
    }
    #[test]
    fn focus_rescan_preserves_edits_selection_and_updates_listing() {
        let (dir, mut app, path) = setup();
        app.session
            .as_mut()
            .unwrap()
            .push_action(image::RgbaImage::new(6, 4), "測試修改");
        let source = app.session.as_ref().unwrap().committed_image();
        image::RgbaImage::new(1, 1)
            .save(dir.path().join("圖片3.png"))
            .unwrap();
        app.refresh_directory(false);
        finish_scan(&mut app);
        assert_eq!(app.playlist.len(), 3);
        assert_eq!(app.playlist.current_file(), Some(&path));
        assert!(Arc::ptr_eq(
            &source,
            &app.session.as_ref().unwrap().committed_image()
        ));
        assert!(app.has_unsaved_changes());
        std::fs::remove_file(&path).unwrap();
        app.refresh_directory(false);
        finish_scan(&mut app);
        assert_eq!(app.playlist.len(), 2);
        assert!(app.playlist.current_index().is_none());
        assert!(app.has_unsaved_changes());
    }
    #[test]
    fn reload_discard_reloads_once_and_cancel_preserves_work() {
        let (_dir, mut app, path) = setup();
        app.session
            .as_mut()
            .unwrap()
            .push_action(image::RgbaImage::new(6, 4), "測試修改");
        app.refresh_directory(true);
        assert!(app.has_pending_leave());
        app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
        assert!(app.has_unsaved_changes());
        image::RgbaImage::new(7, 8).save(path).unwrap();
        app.refresh_directory(true);
        app.resolve_pending_leave(LeaveChoice::Discard).unwrap();
        finish_scan(&mut app);
        assert!(app.wait_document_io(Duration::from_secs(5)));
        assert!(!app.has_pending_leave());
        assert!(!app.has_unsaved_changes());
        assert_eq!(
            app.session.as_ref().unwrap().committed_image().dimensions(),
            (7, 8)
        );
    }
    #[test]
    fn repeated_reload_does_not_turn_later_focus_scan_into_reload() {
        let (_dir, mut app, _path) = setup();
        app.refresh_directory(true);
        app.refresh_directory(true);
        finish_scan(&mut app);
        assert!(app.wait_document_io(Duration::from_secs(5)));
        app.session
            .as_mut()
            .unwrap()
            .push_action(image::RgbaImage::new(9, 8), "測試修改");
        app.refresh_directory(false);
        finish_scan(&mut app);
        assert!(!app.is_loading_image());
        assert!(app.has_unsaved_changes());
        assert_eq!(
            app.session.as_ref().unwrap().committed_image().dimensions(),
            (9, 8)
        );
    }
    #[test]
    fn obsolete_scan_cannot_replace_a_new_document() {
        let (dir, mut app, _path) = setup();
        app.refresh_directory(false);
        let target = dir.path().join("圖片2.png");
        app.open_image_path(target.clone()).unwrap();
        assert!(app.wait_document_io(Duration::from_secs(5)));
        finish_scan(&mut app);
        assert_eq!(app.playlist.current_file(), Some(&target));
    }
    #[test]
    fn delete_guard_cancel_and_failure_preserve_original_and_history() {
        let (_dir, mut app, path) = setup();
        let bytes = std::fs::read(&path).unwrap();
        app.session
            .as_mut()
            .unwrap()
            .push_action(image::RgbaImage::new(6, 4), "測試修改");
        assert!(app.request_delete_current());
        assert!(app.has_pending_leave());
        assert!(!app.file_actions.confirming_delete);
        app.resolve_pending_leave(LeaveChoice::Discard).unwrap();
        assert!(app.file_actions.confirming_delete);
        app.file_actions.skip_confirmation = true;
        app.resolve_delete_current(false);
        assert!(app.preferences.current.confirm_delete);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let selected = app.playlist.current_file().cloned();
        app.finish_recycle(Err("模擬回收桶不可用".into()));
        assert_eq!(app.playlist.current_file(), selected.as_ref());
        assert!(app.has_unsaved_changes());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn successful_recycle_updates_next_selection_then_empty_document() {
        let (_dir, mut app, path) = setup();
        std::fs::remove_file(path).unwrap();
        app.finish_recycle(Ok(()));
        assert_eq!(app.playlist.len(), 1);
        assert!(app.wait_document_io(Duration::from_secs(5)));
        let last = app.playlist.current_file().unwrap().clone();
        std::fs::remove_file(last).unwrap();
        app.finish_recycle(Ok(()));
        assert!(app.playlist.is_empty());
        assert!(app.session.is_none());
    }
    #[test]
    fn missing_file_recycle_fails_without_advancing_selection() {
        let (_dir, mut app, path) = setup();
        std::fs::remove_file(&path).unwrap();
        assert!(app.request_delete_current());
        app.resolve_delete_current(true);
        let start = Instant::now();
        while app.is_recycling() {
            app.poll_file_actions();
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::yield_now();
        }
        assert_eq!(app.playlist.current_file(), Some(&path));
        assert!(app.session.is_some());
    }
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "需原生 Windows 回收桶；只回收並還原測試建立的檔案"]
    fn native_recycle_roundtrip_or_refusal_preserves_fixture() {
        let (_dir, mut app, path) = setup();
        let bytes = std::fs::read(&path).unwrap();
        let canonical = path.canonicalize().unwrap();
        app.request_delete_current();
        app.resolve_delete_current(true);
        let start = Instant::now();
        while app.is_recycling() {
            app.poll_file_actions();
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(10));
        }
        if path.exists() {
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            assert_eq!(app.playlist.current_file(), Some(&path));
            assert!(
                app.status_notification
                    .as_ref()
                    .is_some_and(|(text, _)| text.contains("無法移至回收桶"))
            );
            eprintln!("本環境拒絕回收：原檔與清單保留；成功回收與還原仍待原生桌面驗收");
            return;
        }
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|item| {
                crate::recent::same_path(&item.original_parent, canonical.parent().unwrap())
                    || crate::recent::same_path(&item.original_path(), &canonical)
                    || crate::recent::same_path(&item.original_path(), &path)
            })
            .collect();
        assert_eq!(items.len(), 1);
        trash::os_limited::restore_all(items).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
