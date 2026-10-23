use image::{Rgba, RgbaImage};
use std::sync::Arc;
use viewer_state::{EditingSession, history::HistoryManager};

#[test]
fn budget_deduplicates_and_preserves_saved_identity() {
    let original = Arc::new(RgbaImage::new(2, 2));
    let mut h = HistoryManager::new(original.clone(), "開啟");
    h.set_budget(32);
    h.push(original, "相同配置");
    assert_eq!(h.retained_bytes(), 16);
    h.push(Arc::new(RgbaImage::new(2, 2)), "第二配置");
    h.mark_saved();
    h.push(Arc::new(RgbaImage::new(2, 2)), "第三配置");
    assert_eq!(h.retained_bytes(), 32);
    assert!(!h.can_undo());
    assert!(h.is_dirty());
    h.set_budget(1);
    assert_eq!(h.retained_bytes(), 32);
}

#[test]
fn undo_redo_and_geometry_drafts_preserve_semantics() {
    let mut s = EditingSession::new(RgbaImage::new(2, 2), None);
    let identity = s.identity();
    let preview = Arc::new(RgbaImage::from_pixel(1, 1, Rgba([2, 3, 4, 255])));
    s.set_geometry_preview(preview.clone());
    assert!(s.is_dirty());
    assert!(!s.is_committed_dirty());
    assert_eq!(s.identity(), identity);
    s.cancel_adjustments();
    assert!(!s.is_dirty());
    s.set_geometry_preview(preview);
    assert!(s.apply_geometry_preview("裁切"));
    assert_eq!(s.committed_image().dimensions(), (1, 1));
    assert!(s.undo());
    assert_eq!(s.identity(), identity);
    assert!(!s.is_dirty());
    assert!(s.redo());
    assert!(s.is_dirty());
}

#[test]
fn reducing_budget_evicts_farthest_redo_without_changing_saved_node() {
    let mut h = HistoryManager::new(Arc::new(RgbaImage::new(2, 2)), "原圖");
    h.push(Arc::new(RgbaImage::new(2, 2)), "第一步");
    h.mark_saved();
    h.push(Arc::new(RgbaImage::new(2, 2)), "第二步");
    h.push(Arc::new(RgbaImage::new(2, 2)), "第三步");
    h.undo();
    h.undo();
    assert!(!h.is_dirty());
    h.set_budget(48);
    assert_eq!(h.retained_bytes(), 48);
    assert_eq!(h.redo_len(), 1);
    h.redo();
    assert!(h.is_dirty());
    assert!(!h.can_redo());
    h.undo();
    assert!(!h.is_dirty());
}
