use app_ui::app::ImageViewerApp;
use image::{Rgba, RgbaImage};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

fn make_zip(path: &Path) {
    let mut bytes = std::io::Cursor::new(Vec::new());
    RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 255]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    zip.start_file("old.png", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(bytes.get_ref()).unwrap();
    zip.finish().unwrap();
}

fn drain_result(app: &mut ImageViewerApp) {
    let start = Instant::now();
    while !app.poll_archive_loader() {
        assert!(start.elapsed() < Duration::from_secs(5), "背景結果未到達");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_archive_started(app: &mut ImageViewerApp) {
    let start = Instant::now();
    while app.latest_archive_req_id == 0 {
        app.poll_document_io();
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "背景路徑辨識未完成"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn newer_image_survives_old_archive_result() {
    let dir = tempfile::tempdir().unwrap();
    let image_path = dir.path().join("new.png");
    RgbaImage::from_pixel(8, 8, Rgba([0, 255, 0, 255]))
        .save(&image_path)
        .unwrap();
    let archive_path = dir.path().join("old.zip");
    make_zip(&archive_path);
    let mut app = ImageViewerApp::default();
    app.open_file_path(archive_path).unwrap();
    wait_archive_started(&mut app);
    app.open_file_path(image_path.clone()).unwrap();
    assert!(app.wait_document_io(Duration::from_secs(5)));
    drain_result(&mut app);
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(image_path.as_path())
    );
    assert!(app.archive_session.is_none());
}

#[test]
fn cancelled_archive_does_not_replace_session() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.zip");
    make_zip(&path);
    let mut app = ImageViewerApp::default();
    app.open_file_path(path).unwrap();
    wait_archive_started(&mut app);
    app.cancel_archive_loading();
    drain_result(&mut app);
    assert!(app.session.is_none());
    assert!(!app.show_password_dialog);
}

#[test]
fn old_error_does_not_interrupt_new_archive() {
    let dir = tempfile::tempdir().unwrap();
    let latest = dir.path().join("latest.zip");
    make_zip(&latest);
    let mut app = ImageViewerApp::default();
    app.open_file_path(dir.path().join("missing.zip")).unwrap();
    wait_archive_started(&mut app);
    app.open_file_path(latest.clone()).unwrap();
    assert!(app.wait_archive_loaded(Duration::from_secs(5)));
    assert_eq!(app.archive_session.as_ref().unwrap().archive_path, latest);
    assert!(app.archive_password_error.is_none());
}
