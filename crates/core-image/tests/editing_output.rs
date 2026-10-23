use core_image::{
    geometry::{CropRect, EditOperation, ResizeFilter},
    output::*,
};
use image::{Rgba, RgbaImage};

#[test]
fn crop_resize_limits_and_transparent_edges() {
    let source = RgbaImage::from_fn(4, 3, |x, y| Rgba([x as u8, y as u8, 0, 255]));
    let result = EditOperation::Crop(CropRect {
        x: 1,
        y: 1,
        width: 3,
        height: 2,
    })
    .apply(&source)
    .unwrap();
    assert_eq!(result.dimensions(), (3, 2));
    assert_eq!(result.get_pixel(0, 0), source.get_pixel(1, 1));
    assert!(
        EditOperation::Crop(CropRect {
            x: u32::MAX,
            y: 0,
            width: 2,
            height: 1
        })
        .apply(&source)
        .is_err()
    );
    for (width, height) in [(0, 1), (u32::MAX, u32::MAX)] {
        assert!(
            EditOperation::Resize {
                width,
                height,
                filter: ResizeFilter::Cubic
            }
            .apply(&source)
            .is_err()
        );
    }
    let transparent = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([0, 0, 255, 255])
        } else {
            Rgba([255, 0, 0, 0])
        }
    });
    let resized = EditOperation::Resize {
        width: 8,
        height: 4,
        filter: ResizeFilter::Cubic,
    }
    .apply(&transparent)
    .unwrap();
    assert!(resized.pixels().all(|p| p[0] == 0));
}

#[test]
fn output_roundtrip_and_failed_encoding_preserve_existing_target() {
    let dir = tempfile::tempdir().unwrap();
    let image = RgbaImage::from_pixel(8, 8, Rgba([255, 0, 0, 0]));
    let png = dir.path().join("輸出.png");
    save_with_options(&image, &png, OutputOptions::default()).unwrap();
    assert_eq!(core_image::load_image(&png).unwrap().to_rgba8(), image);
    let jpeg = dir.path().join("輸出.jpg");
    save_with_options(
        &image,
        &jpeg,
        OutputOptions {
            format: OutputFormat::Jpeg,
            ..Default::default()
        },
    )
    .unwrap();
    let pixel = core_image::load_image(&jpeg)
        .unwrap()
        .to_rgb8()
        .get_pixel(0, 0)
        .0;
    assert!(pixel.iter().all(|&c| c > 250));
    let previous = std::fs::read(&png).unwrap();
    assert!(
        save_with_options(
            &image,
            &png,
            OutputOptions {
                quality: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        core_image::io::write_atomic(&png, |_| Err(core_image::ImageError::InvalidEdit(
            "模擬編碼失敗"
        )))
        .is_err()
    );
    assert_eq!(std::fs::read(&png).unwrap(), previous);
    assert!(
        save_with_options(
            &image,
            &png,
            OutputOptions {
                format: OutputFormat::Jpeg,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(
        flatten(
            &RgbaImage::from_pixel(1, 1, Rgba([255, 0, 0, 128])),
            [0, 0, 0]
        )
        .get_pixel(0, 0)
        .0,
        [128, 0, 0]
    );
}

#[test]
#[cfg(windows)]
fn locked_target_is_preserved_when_atomic_replacement_fails() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("鎖定原檔.png");
    std::fs::write(&path, b"original-file").unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    assert!(save_with_options(&RgbaImage::new(2, 2), &path, OutputOptions::default()).is_err());
    drop(lock);
    assert_eq!(std::fs::read(&path).unwrap(), b"original-file");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
