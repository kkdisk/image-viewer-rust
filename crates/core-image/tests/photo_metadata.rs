//! 使用實際 JPEG／EXIF 容器驗證拍攝時間、方向與不回寫原檔。
use core_image::{exif::*, load_image_from_memory, output::*};
use exif::{Field, In, Tag, Value};
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
use std::io::Cursor;

fn photo(orientation: u16, original: Option<&str>, offset: Option<&str>) -> Vec<u8> {
    let mut fields = vec![Field {
        tag: Tag::Orientation,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![orientation]),
    }];
    for (tag, value) in [
        (Tag::DateTimeOriginal, original),
        (Tag::SubSecTimeOriginal, Some("125")),
        (Tag::OffsetTimeOriginal, offset),
        (Tag::DateTime, Some("2026:10:23 16:30:00")),
        (Tag::GPSLatitudeRef, Some("N")),
    ] {
        if let Some(value) = value {
            fields.push(Field {
                tag,
                ifd_num: In::PRIMARY,
                value: Value::Ascii(vec![value.as_bytes().to_vec()]),
            });
        }
    }
    let mut writer = exif::experimental::Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut tiff = Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let mut jpeg = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(RgbImage::from_fn(4, 2, |x, y| {
        Rgb([(x * 60) as u8, (y * 100) as u8, 90])
    }))
    .write_to(&mut jpeg, ImageFormat::Jpeg)
    .unwrap();
    let mut output = vec![0xff, 0xd8, 0xff, 0xe1];
    output.extend_from_slice(&((tiff.get_ref().len() + 8) as u16).to_be_bytes());
    output.extend_from_slice(b"Exif\0\0");
    output.extend_from_slice(tiff.get_ref());
    output.extend_from_slice(&jpeg.into_inner()[2..]);
    output
}

#[test]
fn original_datetime_subseconds_and_timezone_are_not_file_modification_time() {
    let bytes = photo(6, Some("2024:02:29 12:34:56"), Some("+08:00"));
    let metadata = read_photo_metadata_from_reader(&mut Cursor::new(&bytes));
    assert_eq!(
        metadata.captured_at.as_deref(),
        Some("2024-02-29 12:34:56.125")
    );
    assert_eq!(metadata.datetime_source, Some("拍攝時間"));
    assert_eq!(metadata.timezone.as_deref(), Some("+08:00"));
    assert_eq!(metadata.orientation, Some(6));
    let missing = read_photo_metadata_from_reader(&mut Cursor::new(photo(
        1,
        Some("2024:01:01 01:02:03"),
        None,
    )));
    assert!(missing.timezone.is_none());
    let invalid = read_photo_metadata_from_reader(&mut Cursor::new(photo(
        9,
        Some("2023:02:29 12:34:56"),
        Some("+99:00"),
    )));
    assert_eq!(
        invalid.datetime_source,
        Some("EXIF 修改時間（未記錄拍攝時間）")
    );
    assert!(invalid.orientation.is_none());
    assert!(invalid.timezone.is_none());
    assert_eq!(
        read_photo_metadata_from_reader(&mut Cursor::new(b"invalid-data")),
        PhotoMetadata::default()
    );
}

#[test]
fn all_exif_orientations_are_applied_once_and_output_removes_source_metadata() {
    let dir = tempfile::tempdir().unwrap();
    for orientation in 1..=8 {
        let bytes = photo(orientation, Some("2020:01:02 03:04:05"), None);
        let source = dir.path().join(format!("方向{orientation}.jpg"));
        std::fs::write(&source, &bytes).unwrap();
        let encoded_pixels = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let (width, height) = if orientation >= 5 { (2, 4) } else { (4, 2) };
        let expected = image::RgbaImage::from_fn(width, height, |x, y| {
            let (sx, sy) = match orientation {
                1 => (x, y),
                2 => (3 - x, y),
                3 => (3 - x, 1 - y),
                4 => (x, 1 - y),
                5 => (y, x),
                6 => (y, 1 - x),
                7 => (3 - y, 1 - x),
                8 => (3 - y, x),
                _ => unreachable!(),
            };
            *encoded_pixels.get_pixel(sx, sy)
        });
        let loaded = load_image_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(loaded, expected);
        assert_eq!(
            core_image::load_image(&source).unwrap().to_rgba8(),
            expected
        );
        assert_eq!(std::fs::read(&source).unwrap(), bytes);
        let out = dir.path().join("匯出.png");
        save_with_options(&loaded, &out, OutputOptions::default()).unwrap();
        assert_eq!(core_image::load_image(&out).unwrap().to_rgba8(), expected);
        assert_eq!(read_photo_metadata(&out), PhotoMetadata::default());
        let out = dir.path().join("匯出.jpg");
        save_with_options(
            &loaded,
            &out,
            OutputOptions {
                format: OutputFormat::Jpeg,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(read_photo_metadata(&out), PhotoMetadata::default());
    }
}
