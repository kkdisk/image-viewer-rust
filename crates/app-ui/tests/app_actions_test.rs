use core_image::AdjustmentParams;
use image::{ImageBuffer, Rgba};
use std::fs;

#[test]
fn test_app_undo_redo_and_atomic_save() {
    let temp_dir = std::env::temp_dir().join("image_viewer_test_actions");
    fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("test_save_action.png");

    // 建立 10x10 純紅測試圖
    let img = ImageBuffer::from_pixel(10, 10, Rgba([200u8, 50u8, 50u8, 255u8]));
    img.save(&file_path).unwrap();

    let mut app = app_ui::app::ImageViewerApp::default();
    assert!(app.open_image_path(file_path.clone()).is_ok());

    let session = app.session.as_ref().unwrap();
    assert!(!session.can_undo());
    assert!(!session.can_redo());
    assert!(!session.is_dirty());

    // 模擬微調並套用
    let params = AdjustmentParams {
        brightness: 1.5,
        ..Default::default()
    };
    app.session.as_mut().unwrap().update_preview(params);
    assert!(app.session.as_mut().unwrap().has_active_preview());
    assert!(app.session.as_mut().unwrap().apply_adjustments());

    // 套用後應可 Undo，且為未儲存髒狀態
    assert!(app.session.as_ref().unwrap().can_undo());
    assert!(!app.session.as_ref().unwrap().can_redo());
    assert!(app.session.as_ref().unwrap().is_dirty());

    // 測試 Undo
    app.undo();
    assert!(!app.session.as_ref().unwrap().can_undo());
    assert!(app.session.as_ref().unwrap().can_redo());

    // 測試 Redo
    app.redo();
    assert!(app.session.as_ref().unwrap().can_undo());
    assert!(!app.session.as_ref().unwrap().can_redo());
    assert!(app.session.as_ref().unwrap().is_dirty());

    // 測試原子儲存
    assert!(app.save_current_image());
    assert!(!app.session.as_ref().unwrap().is_dirty());

    // 驗證檔案內容已被原子寫回
    let reloaded = core_image::load_image(&file_path).unwrap().to_rgba8();
    let p = reloaded.get_pixel(0, 0);
    // 亮度提高 1.5 倍，200 * 1.5 -> 255 (截頂)
    assert_eq!(p[0], 255);

    // 清理暫存檔
    let _ = fs::remove_file(file_path);
    let _ = fs::remove_dir(temp_dir);
}
