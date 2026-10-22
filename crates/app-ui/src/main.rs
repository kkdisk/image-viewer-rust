#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_ui::app::ImageViewerApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let (startup, startup_error) = match app_ui::startup::parse_args(std::env::args_os().skip(1)) {
        Ok(options) => (options, None),
        Err(error) => (Default::default(), Some(error)),
    };
    let mut path = app_ui::preferences::config_path();
    let (settings, error) = match path.as_deref().map(app_ui::preferences::load).transpose() {
        Ok(settings) => (settings.unwrap_or_default(), None),
        Err(error) => {
            // 保留損毀或較新版本的原始設定，本次只使用預設值。
            path = None;
            (app_ui::preferences::Preferences::default(), Some(error))
        }
    };
    let native_options = eframe::NativeOptions {
        centered: true,
        viewport: egui::ViewportBuilder::default()
            .with_title("Rust Image Viewer")
            .with_inner_size(settings.window_size)
            .with_maximized(settings.maximized)
            .with_min_inner_size([800.0, 600.0])
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Image Viewer",
        native_options,
        Box::new(move |cc| {
            let mut app = ImageViewerApp::new(cc);
            app.configure_preferences(settings, path);
            if let Some(path) = startup.path
                && let Err(error) = app.open_file_path(path)
            {
                app.status_notification = Some((error, std::time::Instant::now()));
            }
            if let Some(error) = startup_error {
                app.status_notification = Some((error, std::time::Instant::now()));
            }
            if let Some(error) = error {
                app.status_notification = Some((
                    format!("偏好設定無法讀取，本次使用預設值：{error}"),
                    std::time::Instant::now(),
                ));
            }
            Ok(Box::new(app))
        }),
    )
}
