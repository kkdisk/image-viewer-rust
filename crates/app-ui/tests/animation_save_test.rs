use app_ui::{app::ImageViewerApp, navigation_guard::LeaveChoice};
use image::{Frame, Rgba, RgbaImage};
use std::{path::Path, time::Duration};

fn animation(path: &Path) {
    if path
        .extension()
        .is_some_and(|extension| extension == "webp")
    {
        fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
            let mut result = kind.to_vec();
            result.extend_from_slice(&(data.len() as u32).to_le_bytes());
            result.extend_from_slice(data);
            if !data.len().is_multiple_of(2) {
                result.push(0);
            }
            result
        }
        let mut pixels = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut pixels)
            .encode(&[100; 12], 2, 2, image::ExtendedColorType::Rgb8)
            .unwrap();
        let mut body = chunk(b"VP8X", &[2, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
        body.extend(chunk(b"ANIM", &[0; 6]));
        for _ in 0..2 {
            let mut frame = vec![0; 16];
            frame[6] = 1;
            frame[9] = 1;
            frame[12] = 10;
            frame[15] = 2;
            frame.extend_from_slice(&pixels[12..]);
            body.extend(chunk(b"ANMF", &frame));
        }
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());
        bytes.extend_from_slice(b"WEBP");
        bytes.extend(body);
        std::fs::write(path, bytes).unwrap();
        return;
    }
    let mut encoder = image::codecs::gif::GifEncoder::new(std::fs::File::create(path).unwrap());
    for value in [40, 80] {
        encoder
            .encode_frame(Frame::new(RgbaImage::from_pixel(
                2,
                2,
                Rgba([value, 0, 0, 255]),
            )))
            .unwrap();
    }
}

#[test]
fn animated_save_requires_confirmation_and_preserves_source_on_cancel_or_failure() {
    for extension in ["gif", "webp"] {
        verify_animation_save(extension);
    }
}

fn verify_animation_save(extension: &str) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join(format!("source.{extension}"));
    animation(&source);
    let original = std::fs::read(&source).unwrap();
    let mut app = ImageViewerApp::default();
    app.open_image_path(source.clone()).unwrap();
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert!(app.is_animated_source());
    assert!(app.save_current_image());
    assert!(app.has_animation_save_confirmation());
    assert!(!app.resolve_animation_save(false));
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let output = dir.path().join("output.png");
    assert!(app.save_image_to(&output));
    assert!(!output.exists());
    assert!(app.resolve_animation_save(true));
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert!(!app.is_animated_source());
    assert_eq!(image::image_dimensions(output).unwrap(), (2, 2));
    assert_eq!(std::fs::read(&source).unwrap(), original);
    app.open_image_path(source.clone()).unwrap();
    assert!(app.wait_document_io(Duration::from_secs(5)));
    app.session
        .as_mut()
        .unwrap()
        .push_action(RgbaImage::new(2, 2), "編輯");
    assert!(!app.request_close());
    assert!(app.resolve_pending_leave(LeaveChoice::Save).is_ok());
    assert!(app.has_animation_save_confirmation());
    assert!(!app.resolve_animation_save(false));
    assert!(app.has_pending_leave());
    app.resolve_pending_leave(LeaveChoice::Cancel).unwrap();
    let bad_target = dir.path().join("missing").join("output.png");
    assert!(app.save_image_to(&bad_target));
    assert!(app.resolve_animation_save(true));
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert!(app.is_animated_source());
    assert!(app.session.as_ref().unwrap().is_committed_dirty());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert!(app.save_current_image());
    assert!(app.resolve_animation_save(true));
    assert!(app.wait_document_io(Duration::from_secs(5)));
    assert!(!core_image::io::inspect_animation(&source).unwrap());
    assert!(!app.session.as_ref().unwrap().is_dirty());
}
