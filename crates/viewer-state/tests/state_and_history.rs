//! 單元測試：viewer-state 歷程管理、三態模型與視口狀態

use core_image::AdjustmentParams;
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use viewer_state::{EditingSession, HistoryManager, ViewportState};

fn create_solid_image(width: u32, height: u32, color: Rgba<u8>) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = color;
    }
    img
}

#[test]
fn committed_save_state_is_independent_of_preview_and_tracks_history() {
    let mut session = EditingSession::new(
        RgbaImage::from_pixel(2, 2, Rgba([100, 100, 100, 255])),
        None,
    );
    let params = AdjustmentParams {
        brightness: 1.2,
        ..Default::default()
    };
    session.update_preview(params);
    assert!(session.is_dirty());
    assert!(!session.is_committed_dirty());
    session.cancel_adjustments();
    assert!(!session.is_dirty());
    session.update_preview(params);
    assert!(session.apply_adjustments());
    assert!(session.is_committed_dirty());
    session.mark_saved();
    assert!(!session.is_committed_dirty());
    session.update_preview(params);
    assert!(session.is_dirty());
    assert!(!session.is_committed_dirty());
    assert!(session.apply_adjustments());
    assert!(session.is_committed_dirty());
    assert!(session.undo());
    assert!(!session.is_committed_dirty());
    assert!(session.redo());
    assert!(session.is_committed_dirty());
    session.update_preview(params);
    session.cancel_adjustments();
    assert!(session.is_committed_dirty());
}

#[test]
fn test_history_manager_undo_redo_and_limit() {
    let img0 = Arc::new(create_solid_image(10, 10, Rgba([0, 0, 0, 255])));
    let mut history = HistoryManager::new(img0, "初始圖").with_max_steps(3);

    assert_eq!(history.undo_len(), 0);
    assert_eq!(history.redo_len(), 0);
    assert!(!history.is_dirty());

    // 依序推入 4 步 (超過上限 3 步)
    for i in 1..=4 {
        let img = Arc::new(create_solid_image(10, 10, Rgba([i * 10, 0, 0, 255])));
        history.push(img, format!("步驟 {i}"));
        assert!(history.is_dirty());
    }

    // 歷史上限為 3 步，所以 past 只有 3 筆 (第 2, 3, 4 步)
    assert_eq!(history.undo_len(), 3);
    assert_eq!(history.current_description(), "步驟 4");

    // 執行 Undo
    let prev = history.undo().expect("Undo 應成功");
    assert_eq!(prev.get_pixel(0, 0), &Rgba([30, 0, 0, 255]));
    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.redo_len(), 1);

    // 再次 Undo
    let prev2 = history.undo().expect("Undo 應成功");
    assert_eq!(prev2.get_pixel(0, 0), &Rgba([20, 0, 0, 255]));

    // 執行 Redo
    let next = history.redo().expect("Redo 應成功");
    assert_eq!(next.get_pixel(0, 0), &Rgba([30, 0, 0, 255]));
    assert_eq!(history.redo_len(), 1);

    // 在歷史中途推入新動作，應清空 Redo 棧 (分支清除)
    let branch_img = Arc::new(create_solid_image(10, 10, Rgba([99, 99, 99, 255])));
    history.push(branch_img, "新分支");
    assert_eq!(history.redo_len(), 0);
    assert!(!history.can_redo());
}

#[test]
fn test_editing_session_three_state_model() {
    let raw_img = create_solid_image(20, 20, Rgba([100, 100, 100, 255]));
    let mut session = EditingSession::new(raw_img, None);

    assert!(!session.is_dirty());
    assert!(!session.has_active_preview());
    let orig = session.original_image();

    // 1. 調整滑桿但未套用 (預覽態)
    let params = AdjustmentParams {
        temperature: 50,
        tint: 0,
        brightness: 1.2,
        contrast: 1.0,
        saturation: 1.0,
    };
    session.update_preview(params);

    assert!(session.has_active_preview());
    assert!(session.is_dirty());
    // active_image 應為預覽影像，且與 committed 影像不同
    let active = session.active_image();
    let committed = session.committed_image();
    assert_ne!(active.as_raw(), committed.as_raw());
    // 原始底圖絕不受污染
    assert_eq!(orig.as_raw(), session.original_image().as_raw());

    // 2. 取消微調
    session.cancel_adjustments();
    assert!(!session.has_active_preview());
    assert_eq!(session.active_image().as_raw(), committed.as_raw());
    assert!(!session.can_undo()); // 取消微調不新增 Undo 節點

    // 3. 再次調整並套用 (進入歷史)
    session.update_preview(params);
    let applied = session.apply_adjustments();
    assert!(applied);
    assert!(!session.has_active_preview());
    assert!(session.can_undo());
    assert_eq!(session.committed_image().as_raw(), active.as_raw());

    // 4. 執行 Undo
    let undone = session.undo();
    assert!(undone);
    assert_eq!(session.committed_image().as_raw(), orig.as_raw());
}

#[test]
fn test_viewport_zoom_and_pan() {
    let mut vp = ViewportState::new();
    assert_eq!(vp.zoom, 1.0);
    assert_eq!(vp.pan, (0.0, 0.0));

    // 拖曳平移
    vp.pan_by(15.0, -10.0);
    assert_eq!(vp.pan, (15.0, -10.0));

    // 游標錨點縮放
    vp.zoom_at(2.0, 100.0, 100.0);
    assert_eq!(vp.zoom, 2.0);

    // 視口同步測試
    let mut vp2 = ViewportState::new();
    vp.sync_enabled = true;
    vp.sync_to(&mut vp2);
    assert_eq!(vp2.zoom, vp.zoom);
    assert_eq!(vp2.pan, vp.pan);
}
