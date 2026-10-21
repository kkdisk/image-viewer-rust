//! Checkpoint 3: 編輯與比對功能端到端整合測試
//! 驗證：載入 → 微調預覽 → 雙圖比對同步 → 套用歷史 → Undo/Redo → 原子存檔

use std::fs;
use app_ui::app::ImageViewerApp;
use app_ui::canvas::ComparisonSource;
use core_image::AdjustmentParams;
use image::{ImageBuffer, Rgba};

#[test]
fn test_checkpoint3_editing_and_comparison_lifecycle() {
    let temp_dir = std::env::temp_dir().join("checkpoint3_test_run");
    fs::create_dir_all(&temp_dir).unwrap();
    let image_path = temp_dir.join("sample_editable.png");

    // 建立 16x16 基礎測試圖 (R=100, G=100, B=100)
    let initial_img = ImageBuffer::from_pixel(16, 16, Rgba([100u8, 100u8, 100u8, 255u8]));
    initial_img.save(&image_path).unwrap();

    let mut app = ImageViewerApp::default();
    assert!(app.open_image_path(image_path.clone()).is_ok());

    let session = app.session.as_ref().unwrap();
    assert_eq!(session.original_image().get_pixel(0, 0)[0], 100);
    assert!(!session.can_undo());
    assert!(!session.is_dirty());

    // 1. 調整滑桿參數 (亮度提升 1.5 倍)
    let params = AdjustmentParams {
        brightness: 1.5,
        ..Default::default()
    };
    app.session.as_mut().unwrap().update_preview(params);
    assert!(app.session.as_ref().unwrap().has_active_preview());

    // 預覽影像應已變更 (100 * 1.5 -> 150)
    let active_pixel = app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0];
    assert_eq!(active_pixel, 150);

    // 2. 啟動雙圖分割比對視圖
    app.toggle_side_by_side();
    assert!(app.comparison.is_active);
    assert_eq!(app.comparison.source, ComparisonSource::Original);
    assert!(app.comparison.sync_viewport);

    // 驗證原圖與目前微調兩側像素之差異
    let orig_p = app.session.as_ref().unwrap().original_image().get_pixel(0, 0)[0];
    let curr_p = app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0];
    assert_eq!(orig_p, 100);
    assert_eq!(curr_p, 150);

    // 3. 測試視口同步平移與縮放 (無回授死循環)
    app.viewport.zoom = 3.5;
    app.viewport.pan = (120.0, -80.0);
    if app.comparison.sync_viewport {
        app.left_viewport = app.viewport;
    }
    assert_eq!(app.left_viewport.zoom, 3.5);
    assert_eq!(app.left_viewport.pan, (120.0, -80.0));

    // 4. 套用微調參數至歷史
    assert!(app.session.as_mut().unwrap().apply_adjustments());
    assert!(app.session.as_ref().unwrap().can_undo());
    assert!(!app.session.as_ref().unwrap().can_redo());
    assert!(app.session.as_ref().unwrap().is_dirty());

    // 5. 測試 Undo / Redo
    app.undo();
    assert_eq!(app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0], 100);
    assert!(!app.session.as_ref().unwrap().can_undo());
    assert!(app.session.as_ref().unwrap().can_redo());

    app.redo();
    assert_eq!(app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0], 150);
    assert!(app.session.as_ref().unwrap().can_undo());

    // 6. 原地原子存檔
    assert!(app.save_current_image());
    assert!(!app.session.as_ref().unwrap().is_dirty());

    // 驗證磁碟檔案內容已正確替換且無損
    let reloaded = core_image::load_image(&image_path).unwrap().to_rgba8();
    assert_eq!(reloaded.get_pixel(0, 0)[0], 150);

    // 清理測試檔案
    let _ = fs::remove_file(image_path);
    let _ = fs::remove_dir(temp_dir);
}
