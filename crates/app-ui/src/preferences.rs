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
            version: 1,
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
    if std::env::args_os().any(|arg| arg == "--portable") {
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
    if settings.version != 1 {
        return Err("不支援此偏好設定版本".into());
    }
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

fn write(path: &Path, settings: &Preferences) -> Result<(), String> {
    let parent = path.parent().ok_or("設定路徑缺少目錄")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) struct PreferenceWriter {
    sender: Option<mpsc::Sender<Preferences>>,
    errors: mpsc::Receiver<String>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl PreferenceWriter {
    fn new(path: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel::<Preferences>();
        let (errors, error_receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            while let Ok(mut settings) = receiver.recv() {
                while let Ok(newer) = receiver.try_recv() {
                    settings = newer;
                }
                if let Err(error) = write(&path, &settings) {
                    let _ = errors.send(error);
                }
            }
        });
        Self {
            sender: Some(sender),
            errors: error_receiver,
            worker: Some(worker),
        }
    }

    fn request(&self, settings: Preferences) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(settings);
        }
    }
}

impl Drop for PreferenceWriter {
    fn drop(&mut self) {
        self.sender.take();
        // 關閉階段等待最後一份小型設定完成；畫面更新期間不等待磁碟。
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
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
            writer: path.map(PreferenceWriter::new),
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
    pub fn flush(&mut self) {
        if let Some(writer) = &self.writer {
            writer.request(self.current.clone());
        }
        self.last_sent = self.current.clone();
    }
}

impl crate::app::ImageViewerApp {
    pub fn configure_preferences(&mut self, settings: Preferences, path: Option<PathBuf>) {
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
        ctx.input(|input| {
            let viewport = input.viewport();
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
