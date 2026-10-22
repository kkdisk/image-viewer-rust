#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use app_ui::app::ImageViewerApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rust Image Viewer")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_active(true),
        ..Default::default()
    };

    eframe::run_native(
        "Rust Image Viewer",
        native_options,
        Box::new(|cc| Ok(Box::new(ImageViewerApp::new(cc)))),
    )
}
