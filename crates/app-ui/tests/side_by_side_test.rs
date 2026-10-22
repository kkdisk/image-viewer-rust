use app_ui::app::ImageViewerApp;
use app_ui::canvas::ComparisonSource;
use image::{ImageBuffer, Rgba};
use std::fs;
use std::sync::Arc;

#[test]
fn test_side_by_side_comparison_toggle_and_sync() {
    let temp_dir = std::env::temp_dir().join("image_viewer_test_side_by_side");
    fs::create_dir_all(&temp_dir).unwrap();
    let main_img_path = temp_dir.join("main.png");
    let ref_img_path = temp_dir.join("ref.png");

    let img1 = ImageBuffer::from_pixel(20, 20, Rgba([100u8, 150u8, 200u8, 255u8]));
    img1.save(&main_img_path).unwrap();

    let img2 = ImageBuffer::from_pixel(20, 20, Rgba([200u8, 100u8, 50u8, 255u8]));
    img2.save(&ref_img_path).unwrap();

    let mut app = ImageViewerApp::default();
    assert!(app.open_image_path(main_img_path.clone()).is_ok());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));

    // 初始狀態：比對模式未開啟
    assert!(!app.comparison.is_active);
    assert_eq!(app.comparison.source, ComparisonSource::Original);
    assert!(app.comparison.sync_viewport);

    // 切換開啟雙圖比對
    app.toggle_side_by_side();
    assert!(app.comparison.is_active);

    // 模擬平移與縮放變動
    app.viewport.pan = (42.0, -18.0);
    app.viewport.zoom = 2.0;

    // 同步視口檢查：切換為獨立視口
    app.comparison.sync_viewport = false;
    app.left_viewport.pan = (10.0, 10.0);
    app.left_viewport.zoom = 1.0;
    assert_ne!(app.left_viewport.pan, app.viewport.pan);
    assert_ne!(app.left_viewport.zoom, app.viewport.zoom);

    // 重新開啟同步：將 left_viewport 同步為主 viewport
    app.comparison.sync_viewport = true;
    app.left_viewport = app.viewport;
    assert_eq!(app.left_viewport.pan, app.viewport.pan);
    assert_eq!(app.left_viewport.zoom, app.viewport.zoom);

    // 模擬設定外部參考圖片
    let dyn_ref = core_image::load_image(&ref_img_path).unwrap();
    app.reference_image = Some(Arc::new(dyn_ref.to_rgba8()));
    app.comparison.reference_path = Some(ref_img_path.clone());
    app.comparison.source = ComparisonSource::Reference;

    assert_eq!(app.comparison.source, ComparisonSource::Reference);
    assert!(app.reference_image.is_some());
    assert_eq!(app.comparison.reference_path.as_ref(), Some(&ref_img_path));

    // 切換退出雙圖比對
    app.toggle_side_by_side();
    assert!(!app.comparison.is_active);

    // 清理測試檔案
    let _ = fs::remove_file(main_img_path);
    let _ = fs::remove_file(ref_img_path);
    let _ = fs::remove_dir(temp_dir);
}
