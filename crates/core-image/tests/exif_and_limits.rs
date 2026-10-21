//! 單元與整合測試：EXIF 方向校正與像素安全保護限制

use core_image::{
    apply_orientation, check_pixel_limit, inspect_dimensions_from_memory,
    load_image_from_memory, load_image_from_memory_with_limit, ImageError, MAX_SAFE_PIXELS,
};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgb, RgbImage, Rgba};
use std::io::Cursor;

/// 建立一個 2x3 的測試影像，每個像素具備相異顏色以供方向校正驗證
///
/// 佈局 (x, y)：
/// (0,0)=紅    (1,0)=綠
/// (0,1)=藍    (1,1)=黃
/// (0,2)=青    (1,2)=洋紅
fn create_test_image() -> DynamicImage {
    let mut img = RgbImage::new(2, 3);
    img.put_pixel(0, 0, Rgb([255, 0, 0])); // 紅
    img.put_pixel(1, 0, Rgb([0, 255, 0])); // 綠
    img.put_pixel(0, 1, Rgb([0, 0, 255])); // 藍
    img.put_pixel(1, 1, Rgb([255, 255, 0])); // 黃
    img.put_pixel(0, 2, Rgb([0, 255, 255])); // 青
    img.put_pixel(1, 2, Rgb([255, 0, 255])); // 洋紅
    DynamicImage::ImageRgb8(img)
}

#[test]
fn test_exif_orientation_transformations() {
    let base = create_test_image();

    // Orientation 1: 正常 (2x3)
    let o1 = apply_orientation(base.clone(), 1);
    assert_eq!(o1.dimensions(), (2, 3));
    assert_eq!(o1.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
    assert_eq!(o1.get_pixel(1, 0), Rgba([0, 255, 0, 255]));

    // Orientation 2: 水平翻轉 (2x3) -> (0,0) 與 (1,0) 對調
    let o2 = apply_orientation(base.clone(), 2);
    assert_eq!(o2.dimensions(), (2, 3));
    assert_eq!(o2.get_pixel(0, 0), Rgba([0, 255, 0, 255])); // 變為綠
    assert_eq!(o2.get_pixel(1, 0), Rgba([255, 0, 0, 255])); // 變為紅

    // Orientation 3: 旋轉 180 度 (2x3) -> 原 (1,2) 洋紅跑到 (0,0)
    let o3 = apply_orientation(base.clone(), 3);
    assert_eq!(o3.dimensions(), (2, 3));
    assert_eq!(o3.get_pixel(0, 0), Rgba([255, 0, 255, 255]));
    assert_eq!(o3.get_pixel(1, 2), Rgba([255, 0, 0, 255]));

    // Orientation 4: 垂直翻轉 (2x3) -> 原 (0,2) 青跑到 (0,0)
    let o4 = apply_orientation(base.clone(), 4);
    assert_eq!(o4.dimensions(), (2, 3));
    assert_eq!(o4.get_pixel(0, 0), Rgba([0, 255, 255, 255]));
    assert_eq!(o4.get_pixel(0, 2), Rgba([255, 0, 0, 255]));

    // Orientation 5: 水平翻轉後逆時針 90 度 (3x2)
    let o5 = apply_orientation(base.clone(), 5);
    assert_eq!(o5.dimensions(), (3, 2));

    // Orientation 6: 順時針旋轉 90 度 (3x2)
    let o6 = apply_orientation(base.clone(), 6);
    assert_eq!(o6.dimensions(), (3, 2));
    // 原 (0, 2) 旋轉到 (0, 0)
    assert_eq!(o6.get_pixel(0, 0), Rgba([0, 255, 255, 255]));

    // Orientation 7: 水平翻轉後順時針 90 度 (3x2)
    let o7 = apply_orientation(base.clone(), 7);
    assert_eq!(o7.dimensions(), (3, 2));

    // Orientation 8: 逆時針旋轉 90 度 (3x2)
    let o8 = apply_orientation(base.clone(), 8);
    assert_eq!(o8.dimensions(), (3, 2));
    // 原 (1, 0) 綠色旋轉到 (0, 0)
    assert_eq!(o8.get_pixel(0, 0), Rgba([0, 255, 0, 255]));

    // 未知數值（例如 0, 9）：原圖返回
    let o_unknown = apply_orientation(base.clone(), 9);
    assert_eq!(o_unknown.dimensions(), (2, 3));
    assert_eq!(o_unknown.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
}

#[test]
fn test_pixel_limit_check() {
    // 剛好在上限 64M (8192 * 8192 = 67,108,864)
    assert!(check_pixel_limit(8192, 8192, MAX_SAFE_PIXELS).is_ok());

    // 超過 64M (8192 * 8193 = 67,117,056)
    let result = check_pixel_limit(8192, 8193, MAX_SAFE_PIXELS);
    assert!(result.is_err());
    match result {
        Err(ImageError::ImageTooLarge {
            width,
            height,
            total_pixels,
            max_pixels,
        }) => {
            assert_eq!(width, 8192);
            assert_eq!(height, 8193);
            assert_eq!(total_pixels, 67_117_056);
            assert_eq!(max_pixels, MAX_SAFE_PIXELS);
        }
        _ => panic!("預期返回 ImageTooLarge 錯誤"),
    }
}

#[test]
fn test_load_image_with_safety_limits() {
    // 建立一個 50x50 的 PNG 影像 (2,500 像素)
    let img = DynamicImage::new_rgb8(50, 50);
    let mut bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("PNG 編碼失敗");

    // 測試僅讀取標頭尺寸
    let (w, h) = inspect_dimensions_from_memory(&bytes).expect("尺寸讀取失敗");
    assert_eq!((w, h), (50, 50));

    // 限制上限低於 2500 (例如 1000 像素) -> 預期被安全拒絕
    let err = load_image_from_memory_with_limit(&bytes, 1_000).unwrap_err();
    match err {
        ImageError::ImageTooLarge {
            width,
            height,
            total_pixels,
            max_pixels,
        } => {
            assert_eq!((width, height), (50, 50));
            assert_eq!(total_pixels, 2_500);
            assert_eq!(max_pixels, 1_000);
        }
        _ => panic!("預期返回 ImageTooLarge 錯誤"),
    }

    // 限制上限高於 2500 (例如 5000 像素) -> 成功解碼
    let loaded = load_image_from_memory_with_limit(&bytes, 5_000).expect("應成功載入");
    assert_eq!(loaded.dimensions(), (50, 50));

    // 使用預設 64M 上限載入
    let normal_load = load_image_from_memory(&bytes).expect("應成功載入");
    assert_eq!(normal_load.dimensions(), (50, 50));
}

#[test]
fn test_multiple_formats_support() {
    let img = DynamicImage::new_rgb8(10, 10);

    // 1. PNG
    let mut png_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .unwrap();
    let loaded_png = load_image_from_memory(&png_bytes).unwrap();
    assert_eq!(loaded_png.dimensions(), (10, 10));

    // 2. BMP
    let mut bmp_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut bmp_bytes), ImageFormat::Bmp)
        .unwrap();
    let loaded_bmp = load_image_from_memory(&bmp_bytes).unwrap();
    assert_eq!(loaded_bmp.dimensions(), (10, 10));

    // 3. JPEG
    let mut jpeg_bytes = Vec::new();
    img.write_to(&mut Cursor::new(&mut jpeg_bytes), ImageFormat::Jpeg)
        .unwrap();
    let loaded_jpeg = load_image_from_memory(&jpeg_bytes).unwrap();
    assert_eq!(loaded_jpeg.dimensions(), (10, 10));
}

#[test]
fn test_invalid_image_data_handling() {
    let corrupt_data = b"NOT_A_VALID_IMAGE_HEADER_DATA";
    let result = load_image_from_memory(corrupt_data);
    assert!(result.is_err());
}
