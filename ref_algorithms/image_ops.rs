use pyo3::prelude::*;
use rayon::prelude::*;

/// 白平衡調整
/// temp_factor = temp / 100.0, tint_factor = tint / 100.0
#[pyfunction]
pub fn apply_white_balance(
    image_bytes: &[u8],
    _width: u32,
    _height: u32,
    channels: usize,
    temp: i32,
    tint: i32,
) -> PyResult<Vec<u8>> {
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

    // 平行處理每個像素
    result.par_chunks_exact_mut(channels).for_each(|pixel| {
        let r = (pixel[0] as f32 * r_mult).clamp(0.0, 255.0) as u8;
        let g = (pixel[1] as f32 * g_mult).clamp(0.0, 255.0) as u8;
        let b = (pixel[2] as f32 * b_mult).clamp(0.0, 255.0) as u8;
        
        pixel[0] = r;
        pixel[1] = g;
        pixel[2] = b;
    });
    
    Ok(result)
}

/// 細部調整 (亮度、對比、飽和度)
/// brightness, contrast, saturation 為增強因子 (如 1.0 代表不變)
#[pyfunction]
pub fn apply_fine_tune(
    image_bytes: &[u8],
    width: u32,
    height: u32,
    channels: usize,
    brightness: f32,
    contrast: f32,
    saturation: f32,
) -> PyResult<Vec<u8>> {
    let mut result = image_bytes.to_vec();

    // 如果所有因子都接近 1.0，直接回傳
    if (brightness - 1.0).abs() < 1e-4 && 
       (contrast - 1.0).abs() < 1e-4 && 
       (saturation - 1.0).abs() < 1e-4 {
        return Ok(result);
    }

    // 計算平均亮度 (L) 作為對比度的基準點
    // 使用 BT.601 權重：L = 0.299*R + 0.587*G + 0.114*B
    let mut mean_l_val = 127.5; // 預設值，若對比度要調整才去精算
    if (contrast - 1.0).abs() >= 1e-4 {
        let total_l: f64 = image_bytes.par_chunks_exact(channels).map(|p| {
            let r = p[0] as f64 * brightness as f64;
            let g = p[1] as f64 * brightness as f64;
            let b = p[2] as f64 * brightness as f64;
            0.299 * r + 0.587 * g + 0.114 * b
        }).sum();
        mean_l_val = (total_l / (width as f64 * height as f64)) as f32;
    }

    // 平行處理每個像素
    result.par_chunks_exact_mut(channels).for_each(|pixel| {
        let mut r = pixel[0] as f32;
        let mut g = pixel[1] as f32;
        let mut b = pixel[2] as f32;

        // 1. 亮度 (Brightness)
        if (brightness - 1.0).abs() >= 1e-4 {
            r *= brightness;
            g *= brightness;
            b *= brightness;
        }

        // 2. 對比度 (Contrast)
        if (contrast - 1.0).abs() >= 1e-4 {
            r = mean_l_val + (r - mean_l_val) * contrast;
            g = mean_l_val + (g - mean_l_val) * contrast;
            b = mean_l_val + (b - mean_l_val) * contrast;
        }

        // 3. 飽和度 (Saturation)
        if (saturation - 1.0).abs() >= 1e-4 {
            let l = 0.299 * r + 0.587 * g + 0.114 * b;
            r = l + (r - l) * saturation;
            g = l + (g - l) * saturation;
            b = l + (b - l) * saturation;
        }

        // 夾持並轉回 u8
        pixel[0] = r.clamp(0.0, 255.0) as u8;
        pixel[1] = g.clamp(0.0, 255.0) as u8;
        pixel[2] = b.clamp(0.0, 255.0) as u8;
    });

    Ok(result)
}
