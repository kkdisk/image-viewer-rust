//! 單元與整合測試：Rayon 像素運算管線（白平衡、微調）與原子存檔

use core_image::{
    AdjustmentParams, apply_adjustments, apply_adjustments_rgba, apply_fine_tune_raw,
    apply_white_balance_raw, load_image, save_image_atomic,
};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use tempfile::tempdir;

/// 原始演算法之白平衡實作（用於驗證誤差 < 1e-4）
fn ref_white_balance(image_bytes: &[u8], channels: usize, temp: i32, tint: i32) -> Vec<u8> {
    let mut result = image_bytes.to_vec();
    let temp_factor = temp as f32 / 100.0;
    let tint_factor = tint as f32 / 100.0;

    let (r_temp_mult, b_temp_mult) = if temp_factor > 0.0 {
        (1.0 + temp_factor * 0.8, 1.0 - temp_factor * 0.5)
    } else {
        (1.0 + temp_factor * 0.5, 1.0 - temp_factor * 0.8)
    };

    let g_tint_mult = 1.0 + tint_factor * 0.6;
    let rb_tint_mult = 1.0 - tint_factor * 0.1;

    let r_mult = r_temp_mult * rb_tint_mult;
    let g_mult = g_tint_mult;
    let b_mult = b_temp_mult * rb_tint_mult;

    for pixel in result.chunks_exact_mut(channels) {
        let r = (pixel[0] as f32 * r_mult).clamp(0.0, 255.0) as u8;
        let g = (pixel[1] as f32 * g_mult).clamp(0.0, 255.0) as u8;
        let b = (pixel[2] as f32 * b_mult).clamp(0.0, 255.0) as u8;

        pixel[0] = r;
        pixel[1] = g;
        pixel[2] = b;
    }

    result
}

/// 原始演算法之細部微調實作（用於驗證誤差 < 1e-4）
fn ref_fine_tune(
    image_bytes: &[u8],
    width: u32,
    height: u32,
    channels: usize,
    brightness: f32,
    contrast: f32,
    saturation: f32,
) -> Vec<u8> {
    let mut result = image_bytes.to_vec();

    if (brightness - 1.0).abs() < 1e-4
        && (contrast - 1.0).abs() < 1e-4
        && (saturation - 1.0).abs() < 1e-4
    {
        return result;
    }

    let mut mean_l_val = 127.5f32;
    if (contrast - 1.0).abs() >= 1e-4 {
        let mut total_l: f64 = 0.0;
        for p in image_bytes.chunks_exact(channels) {
            let r = p[0] as f64 * brightness as f64;
            let g = p[1] as f64 * brightness as f64;
            let b = p[2] as f64 * brightness as f64;
            total_l += 0.299 * r + 0.587 * g + 0.114 * b;
        }
        mean_l_val = (total_l / (width as f64 * height as f64)) as f32;
    }

    for pixel in result.chunks_exact_mut(channels) {
        let mut r = pixel[0] as f32;
        let mut g = pixel[1] as f32;
        let mut b = pixel[2] as f32;

        if (brightness - 1.0).abs() >= 1e-4 {
            r *= brightness;
            g *= brightness;
            b *= brightness;
        }

        if (contrast - 1.0).abs() >= 1e-4 {
            r = mean_l_val + (r - mean_l_val) * contrast;
            g = mean_l_val + (g - mean_l_val) * contrast;
            b = mean_l_val + (b - mean_l_val) * contrast;
        }

        if (saturation - 1.0).abs() >= 1e-4 {
            let l = 0.299 * r + 0.587 * g + 0.114 * b;
            r = l + (r - l) * saturation;
            g = l + (g - l) * saturation;
            b = l + (b - l) * saturation;
        }

        pixel[0] = r.clamp(0.0, 255.0) as u8;
        pixel[1] = g.clamp(0.0, 255.0) as u8;
        pixel[2] = b.clamp(0.0, 255.0) as u8;
    }

    result
}

#[test]
fn test_white_balance_accuracy() {
    // 構造豐富的 RGB 測試資料 (256x3 像素)
    let mut test_bytes = Vec::new();
    for i in 0..256 {
        test_bytes.push(i as u8);
        test_bytes.push((255 - i) as u8);
        test_bytes.push(((i * 7) % 256) as u8);
    }

    let test_cases = [
        (0, 0),
        (50, -30),
        (-60, 40),
        (100, 100),
        (-100, -100),
        (25, 10),
    ];

    for (temp, tint) in test_cases {
        let parallel_res = apply_white_balance_raw(&test_bytes, 3, temp, tint);
        let ref_res = ref_white_balance(&test_bytes, 3, temp, tint);

        assert_eq!(parallel_res.len(), ref_res.len());
        for (i, (&p_val, &r_val)) in parallel_res.iter().zip(ref_res.iter()).enumerate() {
            let diff = (p_val as f32 - r_val as f32).abs();
            assert!(
                diff < 1e-4,
                "白平衡在 temp={temp}, tint={tint}, index={i} 時誤差過大: parallel={p_val}, ref={r_val}"
            );
        }
    }
}

#[test]
fn test_fine_tune_accuracy() {
    let width = 64;
    let height = 64;
    let mut test_bytes = Vec::new();
    for y in 0..height {
        for x in 0..width {
            test_bytes.push(((x * 4) % 256) as u8);
            test_bytes.push(((y * 4) % 256) as u8);
            test_bytes.push((((x + y) * 2) % 256) as u8);
            test_bytes.push(255); // Alpha
        }
    }

    let test_cases = [
        (1.0, 1.0, 1.0),
        (1.2, 1.1, 0.9),
        (0.8, 1.3, 1.5),
        (0.5, 0.5, 0.5),
        (1.5, 1.5, 1.5),
        (1.0, 1.4, 1.0),
    ];

    for (brightness, contrast, saturation) in test_cases {
        let parallel_res = apply_fine_tune_raw(
            &test_bytes,
            width,
            height,
            4,
            brightness,
            contrast,
            saturation,
        );
        let ref_res = ref_fine_tune(
            &test_bytes,
            width,
            height,
            4,
            brightness,
            contrast,
            saturation,
        );

        assert_eq!(parallel_res.len(), ref_res.len());
        for (i, (&p_val, &r_val)) in parallel_res.iter().zip(ref_res.iter()).enumerate() {
            let diff = (p_val as f32 - r_val as f32).abs();
            assert!(
                diff < 1e-4,
                "微調在 (b={brightness}, c={contrast}, s={saturation}), index={i} 時誤差過大: {p_val} vs {r_val}"
            );
        }
    }
}

#[test]
fn test_adjustment_params_pipeline() {
    let mut img = RgbaImage::new(10, 10);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([100, 150, 200, 255]);
    }

    // 1. 預設參數 (Identity) 應無變化
    let default_params = AdjustmentParams::default();
    assert!(default_params.is_identity());
    let unchanged = apply_adjustments_rgba(&img, &default_params);
    assert_eq!(unchanged.as_raw(), img.as_raw());

    // 2. 微調參數套用至 DynamicImage
    let dyn_img = DynamicImage::ImageRgba8(img.clone());
    let custom_params = AdjustmentParams {
        temperature: 20,
        tint: -10,
        brightness: 1.1,
        contrast: 1.05,
        saturation: 1.2,
    };
    assert!(!custom_params.is_identity());

    let adjusted = apply_adjustments(&dyn_img, &custom_params);
    assert_eq!(adjusted.dimensions(), (10, 10));
    assert_ne!(adjusted.to_rgba8().as_raw(), img.as_raw());
}

#[test]
fn test_save_image_atomic() {
    let dir = tempdir().expect("建立暫存目錄失敗");
    let target_file = dir.path().join("test_output.png");

    // 建立 30x30 測試影像
    let mut img = RgbaImage::new(30, 30);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        *pixel = Rgba([x as u8 * 8, y as u8 * 8, 128, 255]);
    }
    let dyn_img = DynamicImage::ImageRgba8(img.clone());

    // 1. 首次原子寫檔
    save_image_atomic(&dyn_img, &target_file).expect("首次原子存檔應成功");
    assert!(target_file.exists());

    // 重新載入驗證
    let loaded1 = load_image(&target_file).expect("讀取儲存後的檔案失敗");
    assert_eq!(loaded1.dimensions(), (30, 30));

    // 2. 第二次覆寫（測試目標檔案已存在時的覆寫原子性）
    let mut img2 = RgbaImage::new(20, 20);
    for pixel in img2.pixels_mut() {
        *pixel = Rgba([255, 0, 0, 255]);
    }
    let dyn_img2 = DynamicImage::ImageRgba8(img2);

    save_image_atomic(&dyn_img2, &target_file).expect("覆寫現有檔案之原子存檔應成功");
    let loaded2 = load_image(&target_file).expect("讀取覆寫後檔案失敗");
    assert_eq!(loaded2.dimensions(), (20, 20));
}

#[test]
fn test_histogram_computation() {
    let mut img = RgbaImage::new(10, 10);
    // 填為純紅 (255, 0, 0, 255)
    for pixel in img.pixels_mut() {
        *pixel = Rgba([255, 0, 0, 255]);
    }

    let hist = core_image::HistogramData::compute_from_rgba(&img);
    assert_eq!(hist.r[255], 100);
    assert_eq!(hist.r[0], 0);
    assert_eq!(hist.g[0], 100);
    assert_eq!(hist.b[0], 100);
    // 純紅感知亮度: (77 * 255) >> 8 = 76
    assert_eq!(hist.luma[76], 100);
    assert_eq!(hist.max_count, 100);
}

#[test]
fn histogram_counts_empty_images_and_pixels_across_parallel_chunk_boundaries() {
    for width in [0, 1, 4095, 4096, 4097, 8193] {
        let img = RgbaImage::from_fn(width, 1, |x, _| {
            Rgba([
                (x % 256) as u8,
                ((x * 3) % 256) as u8,
                ((x * 7) % 256) as u8,
                (x % 256) as u8,
            ])
        });
        let hist = core_image::HistogramData::compute_from_rgba(&img);
        let mut expected = [[0u32; 256]; 4];
        for pixel in img.pixels() {
            let [r, g, b, _] = pixel.0.map(usize::from);
            expected[0][r] += 1;
            expected[1][g] += 1;
            expected[2][b] += 1;
            expected[3][(77 * r + 150 * g + 29 * b) >> 8] += 1;
        }
        assert_eq!(hist.r, expected[0]);
        assert_eq!(hist.g, expected[1]);
        assert_eq!(hist.b, expected[2]);
        assert_eq!(hist.luma, expected[3]);
        assert_eq!(
            hist.max_count,
            expected.iter().flatten().copied().max().unwrap_or(0)
        );
    }
}
