use app_ui::{app::ImageViewerApp, navigation_guard::LeaveChoice};
use core_image::AdjustmentParams;
use image::{Rgba, RgbaImage};
use std::{sync::Arc, time::Duration};

fn fixture() -> (tempfile::TempDir, ImageViewerApp) {
    let dir = tempfile::tempdir().unwrap();
    for (name, red) in [("a.png", 40), ("b.png", 80), ("c.png", 120)] {
        RgbaImage::from_pixel(4, 4, Rgba([red, 0, 0, 255]))
            .save(dir.path().join(name))
            .unwrap();
    }
    let mut app = ImageViewerApp::default();
    app.open_file_path(dir.path().join("a.png")).unwrap();
    wait(&mut app);
    (dir, app)
}

fn wait(app: &mut ImageViewerApp) {
    assert!(app.wait_document_io(Duration::from_secs(5)));
}

fn edit(app: &mut ImageViewerApp) {
    let session = app.session.as_mut().unwrap();
    session.update_preview(AdjustmentParams {
        brightness: 1.5,
        ..Default::default()
    });
    assert!(session.apply_adjustments());
}

#[test]
fn rapid_navigation_uses_requested_index_and_only_installs_latest() {
    let (dir, mut app) = fixture();
    let original = app.session.as_ref().unwrap().active_image();
    app.navigate_next();
    app.navigate_next();
    assert!(app.is_loading_image());
    assert_eq!(app.playlist.current_index(), Some(0));
    assert!(Arc::ptr_eq(
        &original,
        &app.session.as_ref().unwrap().active_image()
    ));
    wait(&mut app);
    assert_eq!(app.playlist.current_index(), Some(2));
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(dir.path().join("c.png").as_path())
    );
}

#[test]
fn latest_failed_read_preserves_document_and_playlist() {
    let (dir, mut app) = fixture();
    let original = app.session.as_ref().unwrap().active_image();
    app.open_file_path(dir.path().join("b.png")).unwrap();
    app.open_file_path(dir.path().join("missing.png")).unwrap();
    wait(&mut app);
    assert!(Arc::ptr_eq(
        &original,
        &app.session.as_ref().unwrap().active_image()
    ));
    assert_eq!(app.playlist.current_index(), Some(0));
    assert!(
        app.status_notification
            .as_ref()
            .unwrap()
            .0
            .contains("載入失敗")
    );
}

#[test]
fn editing_during_read_requires_confirmation_before_installing() {
    let (dir, mut app) = fixture();
    app.open_file_path(dir.path().join("b.png")).unwrap();
    edit(&mut app);
    wait(&mut app);
    assert!(app.has_pending_leave());
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    assert_eq!(app.playlist.current_index(), Some(0));
    assert!(app.session.as_ref().unwrap().is_dirty());
}

#[test]
fn reference_latest_only_and_failure_preserve_previous_reference() {
    let (dir, mut app) = fixture();
    app.open_reference_image_path(dir.path().join("b.png"))
        .unwrap();
    app.open_reference_image_path(dir.path().join("c.png"))
        .unwrap();
    wait(&mut app);
    assert_eq!(
        app.reference_image.as_ref().unwrap().get_pixel(0, 0)[0],
        120
    );
    let reference = app.reference_image.clone().unwrap();
    app.open_reference_image_path(dir.path().join("missing.png"))
        .unwrap();
    wait(&mut app);
    assert!(Arc::ptr_eq(
        &reference,
        app.reference_image.as_ref().unwrap()
    ));
    assert_eq!(app.playlist.current_index(), Some(0));
    assert_eq!(
        app.comparison.reference_path,
        Some(dir.path().join("c.png"))
    );
}

#[test]
fn save_freezes_navigation_and_history_and_close_waits_for_completion() {
    let (dir, mut app) = fixture();
    edit(&mut app);
    let committed = app.session.as_ref().unwrap().committed_image();
    assert!(app.save_current_image());
    assert!(app.is_saving());
    assert!(app.open_file_path(dir.path().join("b.png")).is_err());
    app.undo();
    assert!(Arc::ptr_eq(
        &committed,
        &app.session.as_ref().unwrap().committed_image()
    ));
    assert!(!app.request_close());
    wait(&mut app);
    assert!(!app.session.as_ref().unwrap().is_dirty());
    assert!(app.request_close());
    assert_eq!(
        core_image::load_image(dir.path().join("a.png"))
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[0],
        60
    );
}

#[test]
fn cancel_leave_during_save_keeps_current_document() {
    let (_dir, mut app) = fixture();
    edit(&mut app);
    app.navigate_next();
    app.resolve_pending_leave(LeaveChoice::Save).unwrap();
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    wait(&mut app);
    assert!(!app.has_pending_leave());
    assert_eq!(app.playlist.current_index(), Some(0));
    assert!(!app.session.as_ref().unwrap().is_dirty());
}

#[test]
fn changed_document_after_save_request_is_never_marked_saved_or_closed() {
    let (_dir, mut app) = fixture();
    edit(&mut app);
    assert!(app.save_current_image());
    assert!(!app.request_close());
    // 模擬同一幀中其他呼叫者改動 Session，確認交付仍核對快照。
    edit(&mut app);
    wait(&mut app);
    assert!(app.session.as_ref().unwrap().is_dirty());
    assert!(app.has_pending_leave());
    assert!(!app.request_close());
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
}
