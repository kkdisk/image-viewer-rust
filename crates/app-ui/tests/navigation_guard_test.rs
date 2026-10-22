use app_ui::{app::ImageViewerApp, navigation_guard::LeaveChoice};
use core_image::AdjustmentParams;
use image::{Rgba, RgbaImage};
use std::sync::Arc;

fn fixture() -> (tempfile::TempDir, ImageViewerApp) {
    let dir = tempfile::tempdir().unwrap();
    for (name, red) in [("a.png", 100), ("b.png", 200)] {
        RgbaImage::from_pixel(4, 4, Rgba([red, 0, 0, 255]))
            .save(dir.path().join(name))
            .unwrap();
    }
    let mut app = ImageViewerApp::default();
    app.open_file_path(dir.path().join("a.png")).unwrap();
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    (dir, app)
}

fn edit(app: &mut ImageViewerApp) {
    let session = app.session.as_mut().unwrap();
    session.update_preview(AdjustmentParams {
        brightness: 1.5,
        ..Default::default()
    });
    assert!(session.apply_adjustments());
}

fn archive_fixture(dir: &std::path::Path) -> std::path::PathBuf {
    use std::io::Write;
    let zip_path = dir.join("gallery.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    zip.start_file("inside.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&std::fs::read(dir.join("b.png")).unwrap())
        .unwrap();
    zip.finish().unwrap();
    zip_path
}

#[test]
fn archive_completion_after_edit_requires_confirmation() {
    let (dir, mut app) = fixture();
    let zip_path = archive_fixture(dir.path());
    app.open_file_path(zip_path).unwrap();
    let start = std::time::Instant::now();
    while app.latest_archive_req_id == 0 {
        app.poll_document_io();
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    edit(&mut app);
    assert!(app.wait_archive_loaded(std::time::Duration::from_secs(5)));
    assert!(app.has_pending_leave());
    assert!(app.session.as_ref().unwrap().is_dirty());
    assert_eq!(app.playlist.current_index(), Some(0));
    assert!(!app.is_current_image_from_archive());
    assert!(app.resolve_pending_leave(LeaveChoice::Discard).unwrap());
    assert!(app.wait_archive_loaded(std::time::Duration::from_secs(5)));
    assert!(app.is_current_image_from_archive());
    assert_eq!(
        app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0],
        200
    );
}

#[test]
fn cancelled_navigation_preserves_image_history_and_index() {
    let (_dir, mut app) = fixture();
    edit(&mut app);
    let edited = app.session.as_ref().unwrap().active_image();
    app.navigate_next();
    assert!(app.has_pending_leave());
    assert_eq!(app.playlist.current_index(), Some(0));
    assert!(Arc::ptr_eq(
        &edited,
        &app.session.as_ref().unwrap().active_image()
    ));
    assert!(!app.resolve_pending_leave(LeaveChoice::Cancel).unwrap());
    assert!(app.session.as_ref().unwrap().is_dirty());
    assert!(app.session.as_ref().unwrap().can_undo());
    assert_eq!(app.playlist.current_index(), Some(0));
    app.navigate_next();
    assert!(app.resolve_pending_leave(LeaveChoice::Discard).unwrap());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert_eq!(app.playlist.current_index(), Some(1));
    assert_eq!(
        app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0],
        200
    );
}

#[test]
fn save_then_navigate_persists_committed_pixels() {
    let (dir, mut app) = fixture();
    edit(&mut app);
    app.navigate_next();
    assert!(!app.resolve_pending_leave(LeaveChoice::Save).unwrap());
    assert!(app.has_pending_leave());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert_eq!(
        core_image::load_image(dir.path().join("a.png"))
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[0],
        150
    );
    assert_eq!(app.playlist.current_index(), Some(1));
    assert!(!app.has_pending_leave());
}

#[test]
fn failed_save_keeps_pending_action_and_unsaved_work() {
    let (dir, mut app) = fixture();
    edit(&mut app);
    app.session
        .as_mut()
        .unwrap()
        .set_source_path(dir.path().join("missing/a.png"));
    app.navigate_next();
    assert!(!app.resolve_pending_leave(LeaveChoice::Save).unwrap());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert!(app.has_pending_leave());
    assert!(app.session.as_ref().unwrap().is_dirty());
    assert_eq!(app.playlist.current_index(), Some(0));
    assert_eq!(
        app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0],
        150
    );
}

#[test]
fn preview_and_queued_slider_changes_cannot_be_silently_saved() {
    let (dir, mut app) = fixture();
    app.slider_params.brightness = 1.5;
    app.open_file_path(dir.path().join("b.png")).unwrap();
    assert!(app.has_pending_leave());
    assert!(app.resolve_pending_leave(LeaveChoice::Save).is_err());
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    app.session
        .as_mut()
        .unwrap()
        .update_preview(app.slider_params);
    assert!(!app.save_image_to(&dir.path().join("preview.png")));
    assert!(!dir.path().join("preview.png").exists());
    app.navigate_next();
    assert!(app.resolve_pending_leave(LeaveChoice::Save).is_err());
    assert_eq!(
        core_image::load_image(dir.path().join("a.png"))
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[0],
        100
    );
    assert!(app.resolve_pending_leave(LeaveChoice::Discard).unwrap());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
}

#[test]
fn close_requires_a_choice_and_cancel_does_not_authorize_later_close() {
    let (_dir, mut app) = fixture();
    assert!(app.request_close());
    edit(&mut app);
    assert!(!app.request_close());
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    assert!(!app.request_close());
    assert!(!app.resolve_pending_leave(LeaveChoice::Save).unwrap());
    assert!(app.has_pending_leave());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert!(app.request_close());
}

#[test]
fn failed_open_after_discard_keeps_the_previous_work() {
    let (dir, mut app) = fixture();
    edit(&mut app);
    app.open_file_path(dir.path().join("missing.png")).unwrap();
    assert!(app.resolve_pending_leave(LeaveChoice::Discard).unwrap());
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert!(app.session.as_ref().unwrap().is_dirty());
    assert_eq!(app.playlist.current_index(), Some(0));
    assert_eq!(
        app.session.as_ref().unwrap().active_image().get_pixel(0, 0)[0],
        150
    );
}

#[test]
fn deferred_archive_keeps_temporary_files_until_cancelled() {
    let (dir, mut app) = fixture();
    let zip_path = archive_fixture(dir.path());
    let archive = core_archive::ManagedArchiveSession::open(zip_path).unwrap();
    let extracted = archive.image_paths[0].clone();
    edit(&mut app);
    app.apply_archive_session(archive).unwrap();
    assert!(extracted.exists());
    assert!(app.has_pending_leave());
    assert!(app.archive_session.is_none());
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    assert!(!extracted.exists());
    assert!(app.session.as_ref().unwrap().is_dirty());
}
