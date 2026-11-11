//! HEIC／HEIF／AVIF 等唯讀來源的儲存流程回歸（T4）。
use crate::app::ImageViewerApp;
use crate::navigation_guard::LeaveChoice;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 建立以 HEIC 為來源、已有一步編輯的文件；來源檔內容為固定位元組，用於確認未被覆寫。
fn heic_document(dir: &Path) -> (ImageViewerApp, PathBuf, Vec<u8>) {
    let source = dir.join("相片.HEIC");
    let original = b"not-really-heic-but-must-stay-untouched".to_vec();
    std::fs::write(&source, &original).unwrap();
    let other = dir.join("下一張.png");
    image::RgbaImage::new(2, 2).save(&other).unwrap();
    let mut app = ImageViewerApp::default();
    app.playlist.set_custom_list(vec![source.clone(), other]);
    app.session = Some(viewer_state::EditingSession::new(
        image::RgbaImage::from_pixel(4, 3, image::Rgba([200, 30, 30, 255])),
        Some(source.clone()),
    ));
    app.session.as_mut().unwrap().push_action(
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 200, 10, 255])),
        "編輯",
    );
    (app, source, original)
}

#[test]
fn 唯讀來源按儲存改開另存且預設同名_jpeg_取消不改變任何狀態() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, source, original) = heic_document(dir.path());
    let identity = app.session.as_ref().unwrap().identity();
    app.test_save_dialog.response = None;
    assert!(!app.save_current_image(), "取消另存即未儲存");
    assert_eq!(
        app.test_save_dialog.requested,
        Some((Some(dir.path().to_path_buf()), Some("相片.jpg".into())))
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(app.session.as_ref().unwrap().identity(), identity);
    assert!(app.has_unsaved_changes());
}

#[test]
fn 唯讀來源另存為_jpeg_後切換到新檔且原檔不變() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, source, original) = heic_document(dir.path());
    let target = dir.path().join("相片.jpg");
    app.test_save_dialog.response = Some(target.clone());
    assert!(app.save_current_image());
    // 沿用既有 JPEG 重新壓縮確認。
    assert!(app.has_animation_save_confirmation());
    assert!(app.resolve_animation_save(true));
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert_eq!(
        std::fs::read(&source).unwrap(),
        original,
        "原 HEIC 不得被覆寫"
    );
    let saved = image::open(&target).unwrap().to_rgba8();
    assert_eq!(saved.dimensions(), (4, 3));
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(target.as_path())
    );
    assert!(!app.has_unsaved_changes());
}

#[test]
fn 另存目標為唯讀格式時拒絕且核心寫入也拒絕() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, source, original) = heic_document(dir.path());
    for name in ["輸出.heic", "輸出.HEIF", "輸出.avif"] {
        let target = dir.path().join(name);
        assert!(!app.save_image_to(&target));
        assert!(!target.exists());
        assert!(
            app.status_notification
                .as_ref()
                .unwrap()
                .0
                .contains("只能讀取")
        );
        let error = core_image::save_image_atomic(
            &image::DynamicImage::ImageRgba8(image::RgbaImage::new(1, 1)),
            &target,
        )
        .unwrap_err();
        assert!(error.to_string().contains("只能讀取"), "{error}");
        assert!(!target.exists());
    }
    assert_eq!(std::fs::read(&source).unwrap(), original);
    // GIF 仍可原地儲存，不屬於唯讀格式。
    assert!(!core_image::formats::is_read_only_extension(Path::new(
        "動畫.gif"
    )));
}

#[test]
fn 儲存並繼續遇到唯讀來源時取消另存即保留文件與離開確認() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, source, original) = heic_document(dir.path());
    let index = app
        .playlist
        .files()
        .iter()
        .position(|p| p.ends_with("下一張.png"))
        .unwrap();
    app.navigate_to(index);
    assert!(app.has_pending_leave());
    app.test_save_dialog.response = None;
    assert!(app.resolve_pending_leave(LeaveChoice::Save).is_err());
    assert!(app.has_pending_leave(), "取消另存不得繼續導覽");
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(source.as_path())
    );
    assert!(app.has_unsaved_changes());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    assert!(!app.has_pending_leave());
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(source.as_path())
    );
}

#[test]
fn 儲存並繼續遇到唯讀來源時另存成功後才繼續導覽() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, source, original) = heic_document(dir.path());
    let next = dir.path().join("下一張.png");
    let index = app
        .playlist
        .files()
        .iter()
        .position(|p| p.ends_with("下一張.png"))
        .unwrap();
    app.navigate_to(index);
    assert!(app.has_pending_leave());
    let target = dir.path().join("相片.jpg");
    app.test_save_dialog.response = Some(target.clone());
    assert!(app.resolve_pending_leave(LeaveChoice::Save).is_ok());
    assert!(app.resolve_animation_save(true));
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert!(target.exists());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    // 儲存交付後才排入下一張的背景載入，需再等一次。
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert!(!app.has_pending_leave());
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(next.as_path())
    );
}
