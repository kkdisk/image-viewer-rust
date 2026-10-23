//! 幾何草稿、文件失效及副本輸出的整合驗證。
use app_ui::{app::ImageViewerApp, geometry_editor::GeometryTool, navigation_guard::LeaveChoice};
use image::{Rgba, RgbaImage};
use std::time::Duration;

fn setup() -> (tempfile::TempDir, ImageViewerApp) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("來源.png");
    RgbaImage::from_fn(12, 8, |x, y| Rgba([x as u8, y as u8, 90, 255]))
        .save(&source)
        .unwrap();
    let mut app = ImageViewerApp::default();
    app.open_file_path(source).unwrap();
    assert!(app.wait_document_io(Duration::from_secs(5)));
    (dir, app)
}

#[test]
fn crop_draft_cancel_apply_and_undo_keep_original_and_save_point() {
    let (_dir, mut app) = setup();
    let original = app.session.as_ref().unwrap().committed_image();
    assert!(app.begin_geometry(GeometryTool::Crop));
    assert!(app.has_unsaved_changes());
    assert!(!app.save_current_image());
    assert!(!app.begin_geometry(GeometryTool::Resize));
    app.cancel_geometry();
    assert!(!app.has_unsaved_changes());
    assert!(app.begin_geometry(GeometryTool::Crop));
    assert!(app.set_crop_rect(core_image::geometry::CropRect {
        x: 2,
        y: 1,
        width: 6,
        height: 4
    }));
    assert!(app.apply_geometry());
    assert!(app.wait_geometry(Duration::from_secs(5)));
    assert!(app.session.as_ref().unwrap().is_committed_dirty());
    assert_eq!(
        app.session.as_ref().unwrap().committed_image().dimensions(),
        (6, 4)
    );
    app.undo();
    assert!(!app.has_unsaved_changes());
    assert_eq!(app.session.as_ref().unwrap().committed_image(), original);
    app.redo();
    assert!(app.has_unsaved_changes());
}

#[test]
fn cancelled_pending_geometry_cannot_reappear_and_export_keeps_source() {
    let (dir, mut app) = setup();
    let identity = app.session.as_ref().unwrap().identity();
    let source_path = app
        .session
        .as_ref()
        .unwrap()
        .source_path()
        .unwrap()
        .to_path_buf();
    assert!(app.begin_geometry(GeometryTool::Resize));
    assert!(app.set_resize_dimensions(6, 4, core_image::geometry::ResizeFilter::Cubic));
    assert!(app.apply_geometry());
    app.cancel_geometry();
    assert!(app.wait_geometry(Duration::from_secs(5)));
    assert_eq!(app.session.as_ref().unwrap().identity(), identity);
    assert!(app.export_copy_to(dir.path().join("副本.png"), Default::default()));
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert_eq!(app.session.as_ref().unwrap().identity(), identity);
    assert_eq!(
        app.session.as_ref().unwrap().source_path(),
        Some(source_path.as_path())
    );
    assert!(!app.has_unsaved_changes());
    assert_eq!(
        image::open(dir.path().join("副本.png")).unwrap().to_rgba8(),
        *app.session.as_ref().unwrap().committed_image()
    );
    assert!(app.begin_geometry(GeometryTool::Crop));
    assert!(!app.request_close());
    assert!(app.resolve_pending_leave(LeaveChoice::Cancel).is_ok());
    assert!(app.has_unsaved_changes());
    app.cancel_geometry();
}

#[test]
fn resized_preview_is_reversible_and_applies_one_history_node() {
    let (_dir, mut app) = setup();
    let identity = app.session.as_ref().unwrap().identity();
    assert!(app.begin_geometry(GeometryTool::Resize));
    assert!(app.set_resize_dimensions(6, 4, core_image::geometry::ResizeFilter::Cubic));
    assert!(app.preview_geometry());
    assert!(app.wait_geometry(Duration::from_secs(5)));
    assert_eq!(
        app.session.as_ref().unwrap().active_image().dimensions(),
        (6, 4)
    );
    assert_eq!(app.session.as_ref().unwrap().identity(), identity);
    assert!(!app.apply_current_adjustments());
    app.cancel_geometry();
    assert_eq!(
        app.session.as_ref().unwrap().active_image().dimensions(),
        (12, 8)
    );
    assert!(!app.has_unsaved_changes());
    assert!(app.begin_geometry(GeometryTool::Resize));
    assert!(app.set_resize_dimensions(3, 2, core_image::geometry::ResizeFilter::Pixel));
    assert!(app.preview_geometry());
    assert!(app.wait_geometry(Duration::from_secs(5)));
    assert!(app.apply_geometry());
    assert_eq!(app.session.as_ref().unwrap().history().undo_len(), 1);
    assert_eq!(
        app.session.as_ref().unwrap().committed_image().dimensions(),
        (3, 2)
    );
    app.undo();
    assert_eq!(app.session.as_ref().unwrap().identity(), identity);
}
