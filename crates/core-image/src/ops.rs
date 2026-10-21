//! 像素運算管線 (白平衡、微調、色彩調整，以 Rayon 平行加速)

use image::{DynamicImage, RgbaImage};
use rayon::prelude::*;

/// 影像微調參數
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdjustmentParams {
    /// 色溫 (-100 ~ 100，預設 0)
    pub temperature: i32,
    /// 色調 (-100 ~ 100，預設 0)
    pub tint: i32,
    /// 亮度 (0.0 ~ 2.0，預設 1.0 代表不變)
    pub brightness: f32,
    /// 對比度 (0.0 ~ 2.0，預設 1.0 代表不變)
    pub contrast: f32,
    /// 飽和度 (0.0 ~ 2.0，預設 1.0 代表不變)
    pub saturation: f32,
}

impl Default for AdjustmentParams {
    fn default() -> Self {
        Self {
            temperature: 0,
            tint: 0,
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.0,
        }
    }
}

impl AdjustmentParams {
    /// 判斷是否所有微調參數皆處於無修改預設值
    pub fn is_identity(&self) -> bool {
        self.temperature == 0
            && self.tint == 0
            && (self.brightness - 1.0).abs() < 1e-4
            && (self.contrast - 1.0).abs() < 1e-4
            && (self.saturation - 1.0).abs() < 1e-4
    }
}

/// 白平衡調整 (原始位元組平行加速版)
///
/// temp 介於 -100 ~ 100，tint 介於 -100 ~ 100
/// channels 通常為 3 (RGB) 或 4 (RGBA)
pub fn apply_white_balance_raw(
    image_bytes: &[u8],
    channels: usize,
    temp: i32,
    tint: i32,
) -> Vec<u8> {
    let mut result = image_bytes.to_vec();

    if temp == 0 && tint == 0 {
        return result;
    }

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

    // Rayon 平行處理每個像素
    result.par_chunks_exact_mut(channels).for_each(|pixel| {
        let r = (pixel[0] as f32 * r_mult).clamp(0.0, 255.0) as u8;
        let g = (pixel[1] as f32 * g_mult).clamp(0.0, 255.0) as u8;
        let b = (pixel[2] as f32 * b_mult).clamp(0.0, 255.0) as u8;

        pixel[0] = r;
        pixel[1] = g;
        pixel[2] = b;
    });

    result
}

/// 細部調整 (亮度、對比、飽和度，原始位元組平行加速版)
///
/// brightness, contrast, saturation 為增強因子 (1.0 代表不變)
pub fn apply_fine_tune_raw(
    image_bytes: &[u8],
    width: u32,
    height: u32,
    channels: usize,
    brightness: f32,
    contrast: f32,
    saturation: f32,
) -> Vec<u8> {
    let mut result = image_bytes.to_vec();

    // 若所有因子皆在 1e-4 內無變更，直接返回
    if (brightness - 1.0).abs() < 1e-4
        && (contrast - 1.0).abs() < 1e-4
        && (saturation - 1.0).abs() < 1e-4
    {
        return result;
    }

    // 計算平均亮度 (L) 作為對比度的基準點
    // 使用 BT.601 權重：L = 0.299*R + 0.587*G + 0.114*B
    let mut mean_l_val = 127.5f32;
    if (contrast - 1.0).abs() >= 1e-4 {
        let total_pixels = (width as f64) * (height as f64);
        if total_pixels > 0.0 {
            let total_l: f64 = image_bytes
                .par_chunks_exact(channels)
                .map(|p| {
                    let r = p[0] as f64 * brightness as f64;
                    let g = p[1] as f64 * brightness as f64;
                    let b = p[2] as f64 * brightness as f64;
                    0.299 * r + 0.587 * g + 0.114 * b
                })
                .sum();
            mean_l_val = (total_l / total_pixels) as f32;
        }
    }

    // Rayon 平行處理每個像素
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

    result
}

/// 對 RgbaImage 套用完整的微調參數管線
pub fn apply_adjustments_rgba(img: &RgbaImage, params: &AdjustmentParams) -> RgbaImage {
    if params.is_identity() {
        return img.clone();
    }

    let width = img.width();
    let height = img.height();
    let channels = 4;

    // 先套用白平衡
    let wb_applied = apply_white_balance_raw(
        img.as_raw(),
        channels,
        params.temperature,
        params.tint,
    );

    // 再套用細部微調 (亮度、對比、飽和度)
    let fine_tuned = apply_fine_tune_raw(
        &wb_applied,
        width,
        height,
        channels,
        params.brightness,
        params.contrast,
        params.saturation,
    );

    RgbaImage::from_raw(width, height, fine_tuned)
        .unwrap_or_else(|| img.clone())
}

/// 對 DynamicImage 套用完整的微調參數管線
pub fn apply_adjustments(img: &DynamicImage, params: &AdjustmentParams) -> DynamicImage {
    if params.is_identity() {
        return img.clone();
    }

    let rgba = img.to_rgba8();
    let adjusted_rgba = apply_adjustments_rgba(&rgba, params);
    DynamicImage::ImageRgba8(adjusted_rgba)
}