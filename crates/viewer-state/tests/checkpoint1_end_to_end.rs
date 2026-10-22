//! Checkpoint 1 端到端整合測試
//! 驗證無 UI 狀態下：載入 → 微調 → 提交歷史 → Undo/Redo → 原子寫檔之完整流程

use core_image::{AdjustmentParams, load_image, save_image_atomic};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use tempfile::tempdir;
use viewer_state::EditingSession;

#[test]
fn test_checkpoint1_end_to_end_workflow() {
    let temp_workspace = tempdir().expect("建立測試暫存目錄失敗");
    let input_path = temp_workspace.path().join("source_photo.png");
    let output_path = temp_workspace.path().join("saved_photo.png");

    // 1. 建立具有測試色彩的原始圖片 (50x50)
    let mut initial_raw = RgbaImage::new(50, 50);
    for (x, y, p) in initial_raw.enumerate_pixels_mut() {
        *p = Rgba([(x * 4) as u8, (y * 4) as u8, 120, 255]);
    }
    let initial_dyn = DynamicImage::ImageRgba8(initial_raw.clone());
    save_image_atomic(&initial_dyn, &input_path).expect("初始化檔案寫入失敗");

    // 2. 透過 core-image 載入並進行安全檢查
    let loaded_dyn = load_image(&input_path).expect("影像載入失敗");
    assert_eq!(loaded_dyn.dimensions(), (50, 50));

    // 3. 建立 EditingSession 進入非破壞式編輯
    let mut session = EditingSession::new(loaded_dyn.to_rgba8(), Some(input_path.clone()));
    assert!(!session.is_dirty());
    assert!(!session.can_undo());

    // 4. 滑桿即時微調 (預覽態)
    let params = AdjustmentParams {
        temperature: 40,
        tint: -20,
        brightness: 1.25,
        contrast: 1.1,
        saturation: 1.3,
    };
    session.update_preview(params);
    assert!(session.has_active_preview());
    assert!(session.is_dirty());

    let preview_img = session.active_image();
    assert_ne!(preview_img.as_raw(), session.committed_image().as_raw());

    // 5. 點擊「套用」提交至歷史
    let applied = session.apply_adjustments();
    assert!(applied);
    assert!(!session.has_active_preview());
    assert!(session.can_undo());
    let step1_committed = session.committed_image();
    assert_eq!(step1_committed.as_raw(), preview_img.as_raw());

    // 6. 點擊「Undo」復原至載入狀態
    let undone = session.undo();
    assert!(undone);
    assert_eq!(
        session.committed_image().as_raw(),
        initial_raw.as_raw(),
        "Undo 後應完全恢復至載入時原圖"
    );

    // 7. 點擊「Redo」重做微調
    let redone = session.redo();
    assert!(redone);
    assert_eq!(session.committed_image().as_raw(), step1_committed.as_raw());

    // 8. 執行原子儲存
    let final_image = DynamicImage::ImageRgba8((*session.committed_image()).clone());
    save_image_atomic(&final_image, &output_path).expect("原子儲存最終影像失敗");
    session.mark_saved();
    assert!(!session.is_dirty());

    // 9. 重新載入儲存結果進行校驗
    let reloaded = load_image(&output_path).expect("重新載入儲存檔失敗");
    assert_eq!(reloaded.dimensions(), (50, 50));
    assert_eq!(reloaded.to_rgba8().as_raw(), step1_committed.as_raw());
}
