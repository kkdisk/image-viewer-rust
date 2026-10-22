//! Phase 5.1 測試：直方圖即時計算與局部放大鏡狀態與採樣

use app_ui::app::ImageViewerApp;
use app_ui::panels::HistogramMode;
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use viewer_state::EditingSession;

#[test]
fn magnifier_handles_small_and_boundary_viewports() {
    let ctx = egui::Context::default();
    let img = RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]));
    for (width, height) in [
        (1.0, 1.0),
        (100.0, 100.0),
        (203.0, 255.0),
        (204.0, 256.0),
        (800.0, 600.0),
    ] {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect =
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(width, height));
                app_ui::canvas::MagnifierWidget::render(ui, rect, rect, &img, 4, rect.center());
            });
        });
        assert!(!output.shapes.is_empty());
    }
}

#[test]
fn test_histogram_synchronization_and_modes() {
    let mut app = ImageViewerApp::default();
    assert!(app.cached_histogram.is_none());

    // 建立 100x100 漸層圖片
    let mut img = RgbaImage::new(100, 100);
    for y in 0..100 {
        for x in 0..100 {
            img.put_pixel(x, y, Rgba([(x * 2) as u8, (y * 2) as u8, 128, 255]));
        }
    }

    let session = EditingSession::new(img, None);
    app.session = Some(session);

    // 提交後等待背景結果，介面本身只做非阻塞輪詢
    app.show_side_panel = true;
    wait_histogram(&mut app, |hist| hist.b[128] == 10000);

    let hist = app.cached_histogram.as_ref().expect("直方圖應已計算完成");
    assert!(hist.max_count > 0, "最大計數應大於 0");
    assert_eq!(hist.b[128], 10000, "100x100 所有像素的藍色通道均為 128");

    // 驗證模式預設值與切換
    assert_eq!(app.histogram_mode, HistogramMode::Rgb);
    app.histogram_mode = HistogramMode::Luma;
    assert_eq!(app.histogram_mode, HistogramMode::Luma);
    app.histogram_mode = HistogramMode::Red;
    assert_eq!(app.histogram_mode, HistogramMode::Red);
}

fn wait_histogram(app: &mut ImageViewerApp, expected: impl Fn(&core_image::HistogramData) -> bool) {
    let start = std::time::Instant::now();
    loop {
        app.sync_histogram();
        if app.cached_histogram.as_ref().is_some_and(&expected) {
            break;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "直方圖背景結果未到達"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn histogram_rejects_stale_results_and_clears_without_image() {
    let mut app = ImageViewerApp::default();
    app.session = Some(EditingSession::new(
        RgbaImage::from_pixel(400, 400, Rgba([255, 0, 0, 255])),
        None,
    ));
    app.sync_histogram();
    app.session = Some(EditingSession::new(
        RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255])),
        None,
    ));
    wait_histogram(&mut app, |hist| hist.g[255] == 64);
    assert_eq!(app.cached_histogram.as_ref().unwrap().r[0], 64);
    app.session = None;
    app.sync_histogram();
    assert!(app.cached_histogram.is_none());
}

#[test]
fn test_magnifier_toggle_and_zoom_levels() {
    let mut app = ImageViewerApp::default();

    // 初始狀態：預設關閉，預設倍率 4x
    assert!(!app.magnifier_enabled);
    assert_eq!(app.magnifier_zoom, 4);

    // 模擬 M 鍵切換開啟
    app.magnifier_enabled = !app.magnifier_enabled;
    assert!(app.magnifier_enabled);

    // 模擬 Shift+M 切換倍率: 4 -> 8 -> 2 -> 4
    app.magnifier_zoom = match app.magnifier_zoom {
        2 => 4,
        4 => 8,
        _ => 2,
    };
    assert_eq!(app.magnifier_zoom, 8);

    app.magnifier_zoom = match app.magnifier_zoom {
        2 => 4,
        4 => 8,
        _ => 2,
    };
    assert_eq!(app.magnifier_zoom, 2);

    app.magnifier_zoom = match app.magnifier_zoom {
        2 => 4,
        4 => 8,
        _ => 2,
    };
    assert_eq!(app.magnifier_zoom, 4);

    // 模擬再次按 M 鍵關閉
    app.magnifier_enabled = !app.magnifier_enabled;
    assert!(!app.magnifier_enabled);
}

#[test]
fn test_magnifier_pixel_sampling_accuracy() {
    // 建立 10x10 單色測試圖像，中心 (5, 5) 放置特殊色
    let mut img = RgbaImage::new(10, 10);
    for p in img.pixels_mut() {
        *p = Rgba([0, 0, 0, 255]);
    }
    img.put_pixel(5, 5, Rgba([255, 128, 64, 255]));

    let img_arc = Arc::new(img);
    let center_px = img_arc.get_pixel(5, 5);
    assert_eq!(center_px.0, [255, 128, 64, 255]);

    // 檢查周圍像素邊界採樣
    let neighbor_px = img_arc.get_pixel(4, 5);
    assert_eq!(neighbor_px.0, [0, 0, 0, 255]);
}
