//! 幻燈片時鐘與順序；以載入完成時間起算停留時間。
use crate::app::ImageViewerApp;
use std::{
    collections::HashSet,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

#[derive(Default)]
pub struct Slideshow {
    pub playing: bool,
    deadline: Option<Instant>,
    awaiting: bool,
    paths: Vec<PathBuf>,
    position: usize,
    attempted: Option<PathBuf>,
    failed: HashSet<PathBuf>,
    pub failures: usize,
}

impl ImageViewerApp {
    pub fn stop_slideshow(&mut self) {
        if self.slideshow.awaiting {
            self.document_io.cancel_main();
        }
        self.slideshow.playing = false;
        self.slideshow.deadline = None;
        self.slideshow.awaiting = false;
    }
    pub fn toggle_slideshow(&mut self, ctx: &egui::Context) -> bool {
        if self.slideshow.playing {
            self.stop_slideshow();
            return true;
        }
        if self.session.is_none()
            || self.playlist.len() < 2
            || self.has_unsaved_changes()
            || self.has_pending_leave()
            || self.is_document_busy()
            || self.is_loading_image()
            || self.file_actions.confirming_delete
            || self.browsing.help
            || self.show_about_dialog
            || self.show_password_dialog
            || self.duplicate_finder_state.is_open
        {
            self.status_notification = Some((
                "幻燈片需要至少兩張圖片；請先完成載入、套用或取消預覽並儲存，關閉對話框後再啟動"
                    .into(),
                Instant::now(),
            ));
            return false;
        }
        let current = self
            .session
            .as_ref()
            .and_then(|s| s.source_path())
            .map(PathBuf::from);
        let mut paths = self.playlist.files().to_vec();
        if self.preferences.current.slideshow_random {
            let mut seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(1, |d| d.as_nanos() as u64);
            for i in (1..paths.len()).rev() {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                paths.swap(i, (seed as usize) % (i + 1));
            }
        }
        if let Some(index) = paths.iter().position(|p| Some(p) == current.as_ref()) {
            paths.rotate_left(index);
        }
        if !self.browsing.fullscreen {
            self.toggle_fullscreen(ctx);
        }
        self.slideshow = Slideshow {
            playing: true,
            paths,
            deadline: Some(Instant::now() + self.slide_interval()),
            ..Default::default()
        };
        true
    }
    fn slide_interval(&self) -> Duration {
        let seconds = self.preferences.current.slideshow_seconds;
        Duration::from_secs(if [2, 3, 5, 10].contains(&seconds) {
            seconds
        } else {
            3
        })
    }
    pub(crate) fn slideshow_loaded(&mut self, success: bool, now: Instant) {
        if !self.slideshow.playing || !self.slideshow.awaiting {
            return;
        }
        self.slideshow.awaiting = false;
        if !success {
            self.slideshow.failures += 1;
            if let Some(path) = self.slideshow.attempted.take() {
                self.slideshow.failed.insert(path);
            }
            self.status_notification = Some((
                format!("幻燈片略過載入失敗：共 {} 張", self.slideshow.failures),
                now,
            ));
        }
        self.slideshow.deadline = Some(
            now + if success {
                self.slide_interval()
            } else {
                Duration::ZERO
            },
        );
    }
    pub fn tick_slideshow(&mut self, now: Instant) {
        if !self.slideshow.playing {
            return;
        }
        if self.has_unsaved_changes()
            || self.has_pending_leave()
            || self.is_document_busy()
            || self.file_actions.confirming_delete
            || self.browsing.help
            || self.show_about_dialog
            || self.show_password_dialog
            || self.duplicate_finder_state.is_open
        {
            self.stop_slideshow();
            return;
        }
        if self.slideshow.awaiting
            || self.is_loading_image()
            || self
                .slideshow
                .deadline
                .is_none_or(|deadline| now < deadline)
        {
            return;
        }
        for _ in 0..self.slideshow.paths.len() {
            let mut next = self.slideshow.position + 1;
            if next >= self.slideshow.paths.len() {
                if !self.preferences.current.slideshow_loop {
                    self.stop_slideshow();
                    return;
                }
                next = 0;
            }
            self.slideshow.position = next;
            let path = self.slideshow.paths[next].clone();
            if self.slideshow.failed.contains(&path) {
                continue;
            }
            self.slideshow.attempted = Some(path.clone());
            self.slideshow.awaiting = true;
            if self.enqueue_image(path, None, false).is_err() {
                self.stop_slideshow();
            }
            return;
        }
        self.stop_slideshow();
    }
    pub(crate) fn update_slideshow(&mut self, ctx: &egui::Context) {
        self.tick_slideshow(Instant::now());
        if self.slideshow.playing {
            ctx.request_repaint_after(
                self.slideshow
                    .deadline
                    .map_or(Duration::from_millis(16), |d| {
                        d.saturating_duration_since(Instant::now())
                    })
                    .min(Duration::from_millis(100)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, ImageViewerApp, egui::Context) {
        let dir = tempfile::tempdir().unwrap();
        for name in ["1.png", "2.png", "3.png"] {
            image::RgbaImage::new(3, 2)
                .save(dir.path().join(name))
                .unwrap();
        }
        let mut app = ImageViewerApp::default();
        app.open_image_path(dir.path().join("1.png")).unwrap();
        assert!(app.wait_document_io(Duration::from_secs(5)));
        (dir, app, egui::Context::default())
    }
    #[test]
    fn clock_waits_for_decode_then_respects_interval_and_manual_navigation() {
        let (_dir, mut app, ctx) = setup();
        assert!(app.toggle_slideshow(&ctx));
        assert!(app.browsing.fullscreen);
        let deadline = app.slideshow.deadline.unwrap();
        app.tick_slideshow(deadline - Duration::from_millis(1));
        assert!(!app.is_loading_image());
        app.tick_slideshow(deadline);
        assert!(app.is_loading_image());
        assert!(app.wait_document_io(Duration::from_secs(5)));
        assert_eq!(app.playlist.current_index(), Some(1));
        assert!(!app.slideshow.awaiting);
        app.navigate_next();
        assert!(!app.slideshow.playing);
        assert!(app.wait_document_io(Duration::from_secs(5)));
        assert_eq!(app.playlist.current_index(), Some(2));
    }
    #[test]
    fn non_looping_playback_stops_and_bad_images_are_skipped_once() {
        let (dir, mut app, ctx) = setup();
        std::fs::write(dir.path().join("2.png"), "損毀圖片").unwrap();
        app.preferences.current.slideshow_loop = false;
        app.toggle_slideshow(&ctx);
        for _ in 0..3 {
            app.tick_slideshow(Instant::now() + Duration::from_secs(30));
            assert!(app.wait_document_io(Duration::from_secs(5)));
        }
        assert_eq!(app.slideshow.failures, 1);
        assert_eq!(app.playlist.current_index(), Some(2));
        assert!(!app.slideshow.playing);
    }
    #[test]
    fn defaults_random_permutation_and_dialog_dirty_guards() {
        let (_dir, mut app, ctx) = setup();
        assert_eq!(app.preferences.current.slideshow_seconds, 3);
        assert!(app.preferences.current.slideshow_loop);
        assert!(!app.preferences.current.slideshow_random);
        for seconds in [2, 3, 5, 10] {
            app.preferences.current.slideshow_seconds = seconds;
            assert_eq!(app.slide_interval(), Duration::from_secs(seconds));
        }
        app.preferences.current.slideshow_random = true;
        app.toggle_slideshow(&ctx);
        let unique: HashSet<_> = app.slideshow.paths.iter().collect();
        assert_eq!(unique.len(), 3);
        assert_eq!(app.slideshow.paths.first(), app.playlist.current_file());
        app.stop_slideshow();
        app.browsing.help = true;
        assert!(!app.toggle_slideshow(&ctx));
        app.browsing.help = false;
        app.session
            .as_mut()
            .unwrap()
            .push_action(image::RgbaImage::new(1, 1), "測試編輯");
        assert!(!app.toggle_slideshow(&ctx));
    }
    #[test]
    fn stopping_pending_slide_cancels_delivery() {
        let (_dir, mut app, ctx) = setup();
        let source = app.session.as_ref().unwrap().committed_image();
        app.toggle_slideshow(&ctx);
        app.tick_slideshow(app.slideshow.deadline.unwrap());
        app.stop_slideshow();
        assert!(app.wait_document_io(Duration::from_secs(5)));
        assert!(std::sync::Arc::ptr_eq(
            &source,
            &app.session.as_ref().unwrap().committed_image()
        ));
    }
}
