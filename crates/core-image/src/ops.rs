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
    let wb_applied =
        apply_white_balance_raw(img.as_raw(), channels, params.temperature, params.tint);

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

    RgbaImage::from_raw(width, height, fine_tuned).unwrap_or_else(|| img.clone())
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

/// 256 色階直方圖數據 (RGB 與感知亮度)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistogramData {
    pub r: [u32; 256],
    pub g: [u32; 256],
    pub b: [u32; 256],
    pub luma: [u32; 256],
    pub max_count: u32,
}

impl Default for HistogramData {
    fn default() -> Self {
        Self {
            r: [0; 256],
            g: [0; 256],
            b: [0; 256],
            luma: [0; 256],
            max_count: 0,
        }
    }
}

type PartialHistogram = ([u32; 256], [u32; 256], [u32; 256], [u32; 256]);

impl HistogramData {
    /// 自 RgbaImage 高速計算 RGB 與亮度直方圖數據（使用 Rayon 平行統計多區塊）
    pub fn compute_from_rgba(img: &RgbaImage) -> Self {
        let raw = img.as_raw();
        let pixel_count = raw.len() / 4;
        if pixel_count == 0 {
            return Self::default();
        }

        // 採用 Rayon chunks 平行統計後合併
        let chunk_size = 4096 * 4;
        let partial_hists: Vec<PartialHistogram> = raw
            .par_chunks(chunk_size)
            .map(|chunk| {
                let mut r = [0u32; 256];
                let mut g = [0u32; 256];
                let mut b = [0u32; 256];
                let mut luma = [0u32; 256];

                let mut pixels = chunk;
                while let Some((pixel, remaining)) = pixels.split_first_chunk::<4>() {
                    pixels = remaining;
                    let pr = pixel[0] as usize;
                    let pg = pixel[1] as usize;
                    let pb = pixel[2] as usize;

                    r[pr] += 1;
                    g[pg] += 1;
                    b[pb] += 1;

                    // 感知亮度：Y = 0.299*R + 0.587*G + 0.114*B (定點近似: (77*R + 150*G + 29*B) >> 8)
                    let py = ((77 * pr + 150 * pg + 29 * pb) >> 8).min(255);
                    luma[py] += 1;
                }

                (r, g, b, luma)
            })
            .collect();

        let mut total_r = [0u32; 256];
        let mut total_g = [0u32; 256];
        let mut total_b = [0u32; 256];
        let mut total_luma = [0u32; 256];

        for (pr, pg, pb, pl) in partial_hists {
            for i in 0..256 {
                total_r[i] += pr[i];
                total_g[i] += pg[i];
                total_b[i] += pb[i];
                total_luma[i] += pl[i];
            }
        }

        let mut max_count = 0u32;
        for i in 0..256 {
            max_count = max_count
                .max(total_r[i])
                .max(total_g[i])
                .max(total_b[i])
                .max(total_luma[i]);
        }

        Self {
            r: total_r,
            g: total_g,
            b: total_b,
            luma: total_luma,
            max_count,
        }
    }
}
