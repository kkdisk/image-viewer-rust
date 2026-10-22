//! 具版本的偏好設定；啟動前讀取，執行期間由背景工作原子儲存。

use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub sort: crate::playlist::SortOptions,
    pub recent: Vec<crate::recent::RecentEntry>,
    pub recent_removed: Vec<crate::recent::RecentEntry>,
    pub version: u32,
    pub window_size: [f32; 2],
    pub maximized: bool,
    pub side_panel: bool,
    pub filmstrip: bool,
    pub magnifier_enabled: bool,
    pub magnifier_zoom: u32,
    pub histogram_mode: u8,
    pub white_balance_open: bool,
    pub fine_tune_open: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: 2,
            sort: Default::default(),
            recent: Vec::new(),
            recent_removed: Vec::new(),
            window_size: [1280.0, 800.0],
            maximized: false,
            side_panel: true,
            filmstrip: true,
            magnifier_enabled: false,
            magnifier_zoom: 4,
            histogram_mode: 0,
            white_balance_open: true,
            fine_tune_open: true,
        }
    }
}

pub fn config_path() -> Option<PathBuf> {
    if crate::startup::parse_args(std::env::args_os().skip(1)).is_ok_and(|args| args.portable) {
        return std::env::current_exe()
            .ok()?
            .parent()
            .map(|dir| dir.join("config.json"));
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|dir| dir.join("rust_image_viewer/config.json"))
}

pub fn load(path: &Path) -> Result<Preferences, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(error.to_string()),
    };
    let mut settings: Preferences =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    if ![1, 2].contains(&settings.version) {
        return Err("不支援此偏好設定版本".into());
    }
    settings.version = 2;
    settings.recent = crate::recent::merge(&settings.recent, &[]);
    settings.recent_removed = crate::recent::merge(&settings.recent_removed, &[]);
    settings.recent.retain(|entry| {
        !settings.recent_removed.iter().any(|removed| {
            crate::recent::same_path(&entry.path, &removed.path)
                && removed.opened_ms >= entry.opened_ms
        })
    });
    settings.window_size[0] = settings.window_size[0].clamp(800.0, 7680.0);
    settings.window_size[1] = settings.window_size[1].clamp(600.0, 4320.0);
    if ![2, 4, 8].contains(&settings.magnifier_zoom) {
        settings.magnifier_zoom = 4;
    }
    if settings.histogram_mode > 4 {
        settings.histogram_mode = 0;
    }
    Ok(settings)
}

#[cfg(test)]
fn write(path: &Path, settings: &Preferences) -> Result<(), String> {
    write_snapshot(path, settings, true)
}

fn write_snapshot(path: &Path, settings: &Preferences, closing: bool) -> Result<(), String> {
    let parent = path.parent().ok_or("設定路徑缺少目錄")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.with_extension("lock"))
        .map_err(|error| error.to_string())?;
    let start = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if start.elapsed() < Duration::from_secs(3) => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(format!("設定鎖定失敗：{error}")),
        }
    }
    let latest = load(path)?;
    let mut merged = if closing {
        settings.clone()
    } else {
        latest.clone()
    };
    merged.recent = crate::recent::merge(&settings.recent, &latest.recent);
    merged.recent_removed = crate::recent::merge(&settings.recent_removed, &latest.recent_removed);
    merged.recent.retain(|entry| {
        !merged.recent_removed.iter().any(|removed| {
            crate::recent::same_path(&entry.path, &removed.path)
                && removed.opened_ms >= entry.opened_ms
        })
    });
    // 背景開啟失敗後才記錄移除時間；鎖內合併防止其他視窗以舊快照復活項目。
    let bytes = serde_json::to_vec_pretty(&merged).map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) struct PreferenceWriter {
    sender: Option<mpsc::Sender<(Preferences, bool)>>,
    errors: mpsc::Receiver<String>,
    worker: Option<std::thread::JoinHandle<()>>,
    last: std::sync::Mutex<Option<Preferences>>,
}

impl PreferenceWriter {
    fn new(path: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel::<(Preferences, bool)>();
        let (errors, error_receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            while let Ok(mut settings) = receiver.recv() {
                while let Ok(newer) = receiver.try_recv() {
                    settings = newer;
                }
                if let Err(error) = write_snapshot(&path, &settings.0, settings.1) {
                    let _ = errors.send(error);
                }
            }
        });
        Self {
            sender: Some(sender),
            errors: error_receiver,
            worker: Some(worker),
            last: std::sync::Mutex::new(None),
        }
    }

    fn request(&self, settings: Preferences) {
        if let Ok(mut last) = self.last.lock() {
            *last = Some(settings.clone());
        }
        if let Some(sender) = &self.sender {
            let _ = sender.send((settings, false));
        }
    }
}

impl PreferenceWriter {
    fn close(&mut self) -> Option<String> {
        if let Ok(last) = self.last.get_mut()
            && let Some(settings) = last.take()
            && let Some(sender) = &self.sender
        {
            let _ = sender.send((settings, true));
        }
        self.sender.take();
        // 關閉階段等待最後一份小型設定完成；畫面更新期間不等待磁碟。
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            return Some("偏好設定背景工作異常停止".into());
        }
        None
    }
    fn finish(mut self) -> Option<String> {
        self.close().or_else(|| self.errors.try_iter().last())
    }
}
impl Drop for PreferenceWriter {
    fn drop(&mut self) {
        if let Some(error) = self.close() {
            eprintln!("{error}");
        }
        for error in self.errors.try_iter() {
            eprintln!("{error}");
        }
    }
}

pub(crate) struct PreferenceState {
    pub current: Preferences,
    writer: Option<PreferenceWriter>,
    last_sent: Preferences,
    changed_at: Instant,
}

impl Default for PreferenceState {
    fn default() -> Self {
        Self::new(Preferences::default(), None)
    }
}

impl PreferenceState {
    pub fn needs_flush(&self) -> bool {
        self.writer.is_some() && self.current != self.last_sent
    }
    pub fn new(settings: Preferences, path: Option<PathBuf>) -> Self {
        Self {
            current: settings.clone(),
            writer: path.map(|path| {
                let writer = PreferenceWriter::new(path);
                writer.request(settings.clone());
                writer
            }),
            last_sent: settings,
            changed_at: Instant::now(),
        }
    }
    pub fn update(&mut self, settings: Preferences) -> Option<String> {
        if self.current != settings {
            self.current = settings;
            self.changed_at = Instant::now();
        }
        if self.changed_at.elapsed() >= Duration::from_millis(500) && self.current != self.last_sent
        {
            self.flush();
        }
        self.writer
            .as_ref()
            .and_then(|writer| writer.errors.try_recv().ok())
    }
    pub fn finish(&mut self) -> Option<String> {
        self.flush();
        self.writer.take().and_then(PreferenceWriter::finish)
    }
    pub fn flush(&mut self) {
        if let Some(writer) = &self.writer {
            writer.request(self.current.clone());
        }
        self.last_sent = self.current.clone();
    }
}

impl crate::app::ImageViewerApp {
    pub(crate) fn record_recent(&mut self, path: PathBuf) {
        let opened_ms = crate::recent::timestamp().max(
            self.preferences
                .current
                .recent
                .iter()
                .chain(self.preferences.current.recent_removed.iter())
                .map(|entry| entry.opened_ms)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        );
        self.preferences.current.recent = crate::recent::merge(
            &[crate::recent::RecentEntry { path, opened_ms }],
            &self.preferences.current.recent,
        );
    }

    pub(crate) fn remove_recent(&mut self, path: PathBuf) {
        let opened_ms = crate::recent::timestamp().max(
            self.preferences
                .current
                .recent
                .iter()
                .chain(&self.preferences.current.recent_removed)
                .map(|entry| entry.opened_ms)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        );
        self.preferences
            .current
            .recent
            .retain(|entry| !crate::recent::same_path(&entry.path, &path));
        self.preferences.current.recent_removed = crate::recent::merge(
            &[crate::recent::RecentEntry { path, opened_ms }],
            &self.preferences.current.recent_removed,
        );
    }

    pub fn set_sort_options(&mut self, options: crate::playlist::SortOptions) {
        self.playlist.set_sort(options);
        self.preferences.current.sort = options;
    }
    pub fn configure_preferences(&mut self, settings: Preferences, path: Option<PathBuf>) {
        self.playlist.set_sort(settings.sort);
        self.show_side_panel = settings.side_panel;
        self.show_filmstrip = settings.filmstrip;
        self.magnifier_enabled = settings.magnifier_enabled;
        self.magnifier_zoom = settings.magnifier_zoom;
        self.white_balance_open = settings.white_balance_open;
        self.fine_tune_open = settings.fine_tune_open;
        self.histogram_mode = match settings.histogram_mode {
            1 => crate::panels::HistogramMode::Red,
            2 => crate::panels::HistogramMode::Green,
            3 => crate::panels::HistogramMode::Blue,
            4 => crate::panels::HistogramMode::Luma,
            _ => crate::panels::HistogramMode::Rgb,
        };
        self.preferences = PreferenceState::new(settings, path);
    }

    pub(crate) fn update_preferences(&mut self, ctx: &egui::Context) {
        let mut settings = self.preferences.current.clone();
        settings.side_panel = self.show_side_panel;
        settings.filmstrip = self.show_filmstrip;
        settings.magnifier_enabled = self.magnifier_enabled;
        settings.magnifier_zoom = self.magnifier_zoom;
        settings.white_balance_open = self.white_balance_open;
        settings.fine_tune_open = self.fine_tune_open;
        settings.histogram_mode = match self.histogram_mode {
            crate::panels::HistogramMode::Rgb => 0,
            crate::panels::HistogramMode::Red => 1,
            crate::panels::HistogramMode::Green => 2,
            crate::panels::HistogramMode::Blue => 3,
            crate::panels::HistogramMode::Luma => 4,
        };
        if !self.browsing.fullscreen {
            ctx.input(|input| {
                let viewport = input.viewport();
                if viewport.fullscreen.unwrap_or(false) {
                    return;
                }
                settings.maximized = viewport.maximized.unwrap_or(settings.maximized);
                if !settings.maximized
                    && !viewport.minimized.unwrap_or(false)
                    && let Some(rect) = viewport.inner_rect
                    && rect.width() > 0.0
                    && rect.height() > 0.0
                {
                    settings.window_size = [rect.width(), rect.height()];
                }
            });
        }
        if let Some(error) = self.preferences.update(settings) {
            self.status_notification = Some((format!("無法儲存偏好設定：{error}"), Instant::now()));
        }
        if self.preferences.needs_flush() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn spawn_child(dir: &Path, role: &str) -> ChildGuard {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "preferences::tests::preference_child",
                "--nocapture",
            ])
            .env("IMAGE_VIEWER_PREF_ROLE", role)
            .env("IMAGE_VIEWER_PREF_DIR", dir)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        ChildGuard(child)
    }
    fn wait_file(path: &Path) {
        let start = Instant::now();
        while !path.exists() {
            assert!(start.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn preference_child() {
        let Ok(role) = std::env::var("IMAGE_VIEWER_PREF_ROLE") else {
            return;
        };
        let dir = PathBuf::from(std::env::var_os("IMAGE_VIEWER_PREF_DIR").unwrap());
        let path = dir.join("config.json");
        if role == "hold" {
            let lock = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(path.with_extension("lock"))
                .unwrap();
            lock.lock().unwrap();
            std::fs::write(dir.join("hold.ready"), []).unwrap();
            loop {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        std::fs::write(dir.join(format!("{role}.ready")), []).unwrap();
        wait_file(&dir.join("start"));
        let settings = Preferences {
            side_panel: role == "b",
            recent: vec![crate::recent::RecentEntry {
                path: dir.join(format!("{role}.png")),
                opened_ms: if role == "a" { 1 } else { 2 },
            }],
            ..Default::default()
        };
        write_snapshot(&path, &settings, true).unwrap();
    }
    #[test]
    fn processes_merge_recent_and_recover_abandoned_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(dir.path().join("a.png"), []).unwrap();
        std::fs::write(dir.path().join("b.png"), []).unwrap();
        let mut a = spawn_child(dir.path(), "a");
        let mut b = spawn_child(dir.path(), "b");
        wait_file(&dir.path().join("a.ready"));
        wait_file(&dir.path().join("b.ready"));
        std::fs::write(dir.path().join("start"), []).unwrap();
        assert!(a.0.wait().unwrap().success());
        assert!(b.0.wait().unwrap().success());
        assert_eq!(load(&path).unwrap().recent.len(), 2);
        let original = std::fs::read(&path).unwrap();
        let mut holder = spawn_child(dir.path(), "hold");
        wait_file(&dir.path().join("hold.ready"));
        assert!(write_snapshot(&path, &Preferences::default(), true).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        holder.0.kill().unwrap();
        holder.0.wait().unwrap();
        write_snapshot(
            &path,
            &Preferences {
                side_panel: false,
                ..Default::default()
            },
            true,
        )
        .unwrap();
        assert!(!load(&path).unwrap().side_panel);
    }
    #[test]
    fn migration_and_last_close_win_over_regular_writes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"version":1,"side_panel":false}"#).unwrap();
        assert_eq!(load(&path).unwrap().version, 2);
        write_snapshot(&path, &Preferences::default(), false).unwrap();
        assert!(!load(&path).unwrap().side_panel);
        write_snapshot(&path, &Preferences::default(), true).unwrap();
        assert!(load(&path).unwrap().side_panel);
    }

    #[test]
    fn stale_process_cannot_restore_removed_recent_item() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let image = dir.path().join("bad.png");
        std::fs::write(&image, []).unwrap();
        let stale = Preferences {
            recent: vec![crate::recent::RecentEntry {
                path: image.clone(),
                opened_ms: 1,
            }],
            ..Default::default()
        };
        write_snapshot(&path, &stale, true).unwrap();
        let removed = Preferences {
            recent_removed: vec![crate::recent::RecentEntry {
                path: image,
                opened_ms: 2,
            }],
            ..Default::default()
        };
        write_snapshot(&path, &removed, true).unwrap();
        write_snapshot(&path, &stale, false).unwrap();
        assert!(load(&path).unwrap().recent.is_empty());
    }

    #[test]
    fn fullscreen_viewport_never_overwrites_normal_window_size() {
        let mut app = crate::app::ImageViewerApp::default();
        let original = app.preferences.current.window_size;
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.fullscreen = Some(true);
        viewport.inner_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(3840.0, 2160.0),
        ));
        let _ = ctx.run(input, |ctx| app.update_preferences(ctx));
        assert_eq!(app.preferences.current.window_size, original);
    }

    #[test]
    fn preferences_round_trip_and_last_write_survive_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings/config.json");
        let writer = PreferenceWriter::new(path.clone());
        writer.request(Preferences::default());
        let expected = Preferences {
            side_panel: false,
            magnifier_zoom: 8,
            maximized: true,
            ..Default::default()
        };
        writer.request(expected.clone());
        drop(writer);
        assert_eq!(load(&path).unwrap(), expected);
    }

    #[test]
    fn malformed_and_future_settings_are_not_silently_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        assert_eq!(load(&path).unwrap(), Preferences::default());
        for bytes in ["invalid", "{\"version\":99}"] {
            std::fs::write(&path, bytes).unwrap();
            assert!(load(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
        }
        std::fs::write(
            &path,
            "{\"window_size\":[10,99999],\"magnifier_zoom\":9,\"histogram_mode\":8}",
        )
        .unwrap();
        let settings = load(&path).unwrap();
        assert_eq!(settings.window_size, [800.0, 4320.0]);
        assert_eq!(settings.magnifier_zoom, 4);
        assert_eq!(settings.histogram_mode, 0);
    }

    #[test]
    fn app_preferences_restore_and_capture_controls_and_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let settings = Preferences {
            side_panel: false,
            filmstrip: false,
            magnifier_enabled: true,
            magnifier_zoom: 8,
            histogram_mode: 4,
            white_balance_open: false,
            fine_tune_open: false,
            window_size: [1000.0, 700.0],
            ..Default::default()
        };
        let mut app = crate::app::ImageViewerApp::default();
        app.configure_preferences(settings.clone(), Some(path.clone()));
        assert!(!app.show_side_panel && !app.show_filmstrip);
        assert!(app.magnifier_enabled);
        assert_eq!(app.magnifier_zoom, 8);
        assert_eq!(app.histogram_mode, crate::panels::HistogramMode::Luma);
        assert!(!app.white_balance_open && !app.fine_tune_open);
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .inner_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 750.0),
        ));
        let _ = ctx.run(input, |ctx| app.update_preferences(ctx));
        app.preferences.flush();
        drop(app);
        assert_eq!(
            load(&path).unwrap(),
            Preferences {
                window_size: [1100.0, 750.0],
                ..settings
            }
        );
    }

    #[test]
    fn failed_atomic_write_preserves_existing_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        write(&path, &Preferences::default()).unwrap();
        let original = std::fs::read(&path).unwrap();
        // 以讀取分享模式鎖定目標，驗證 Windows 替換失敗不損毀原檔。
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let _lock = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&path)
                .unwrap();
            assert!(
                write(
                    &path,
                    &Preferences {
                        side_panel: false,
                        ..Default::default()
                    }
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        #[cfg(not(windows))]
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
