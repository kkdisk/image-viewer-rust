//! 檔案雜湊 (MD5) 與感知雜湊 (pHash / DCT) 計算模組

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use image::DynamicImage;
use md5::{Digest, Md5};
use rayon::prelude::*;

use crate::error::DuplicateError;

/// 預設 pHash 低頻方塊邊長 (8x8 產生 64-bit 雜湊)
pub const DEFAULT_HASH_SIZE: u32 = 8;

/// 計算單一檔案之 MD5 雜湊值 (以 64KB 緩衝區串流讀取，不佔用巨量記憶體)
pub fn compute_md5<P: AsRef<Path>>(path: P) -> Result<String, DuplicateError> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref).map_err(|e| DuplicateError::IoError {
        path: path_ref.to_path_buf(),
        source: e,
    })?;

    let mut reader = BufReader::with_capacity(65536, file);
    let mut hasher = Md5::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| DuplicateError::IoError {
                path: path_ref.to_path_buf(),
                source: e,
            })?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// 計算單一圖片檔案之感知雜湊 (pHash)
///
/// 整合 core-image 安全載入與 EXIF 方向校正，避免旋轉標籤影響雜湊一致性。
pub fn compute_phash<P: AsRef<Path>>(
    path: P,
    hash_size: u32,
) -> Result<Vec<u8>, DuplicateError> {
    let path_ref = path.as_ref();
    let dyn_img = core_image::load_image(path_ref).map_err(|e| DuplicateError::DecodeError {
        path: path_ref.to_path_buf(),
        message: e.to_string(),
    })?;

    Ok(compute_phash_from_image(&dyn_img, hash_size))
}

/// 自記憶體中之 DynamicImage 計算感知雜湊 (pHash)
///
/// 流程：
/// 1. 將圖片縮放至 (hash_size * 4) x (hash_size * 4)，使用平滑濾波
/// 2. 轉為灰階圖 (Luma8)
/// 3. 計算 2D DCT (Type-II) 頻譜矩陣
/// 4. 取左上 hash_size x hash_size 低頻區塊（跳過 DC [0,0] 分量）
/// 5. 計算低頻分量之中位數，進行二值化並打包為 bytes
pub fn compute_phash_from_image(img: &DynamicImage, hash_size: u32) -> Vec<u8> {
    let resize_dim = hash_size * 4;
    let resized = img.resize_exact(resize_dim, resize_dim, image::imageops::FilterType::Triangle);
    let gray = resized.to_luma8();

    let n = resize_dim as usize;
    let pixels: Vec<f64> = gray.pixels().map(|p| p.0[0] as f64).collect();

    // 計算 2D DCT
    let dct = dct_2d(&pixels, n, n);

    let hs = hash_size as usize;
    let mut low_freq: Vec<f64> = Vec::with_capacity(hs * hs - 1);
    for y in 0..hs {
        for x in 0..hs {
            if x == 0 && y == 0 {
                continue; // 排除 DC 分量
            }
            low_freq.push(dct[y * n + x]);
        }
    }

    let median = compute_median(&low_freq);

    // 二值化與 bit 打包
    let total_bits = hs * hs;
    let num_bytes = total_bits.div_ceil(8);
    let mut hash_bytes = vec![0u8; num_bytes];

    let mut bit_idx = 0;
    for y in 0..hs {
        for x in 0..hs {
            let val = dct[y * n + x];
            if val > median {
                let byte_idx = bit_idx / 8;
                let bit_pos = 7 - (bit_idx % 8);
                hash_bytes[byte_idx] |= 1 << bit_pos;
            }
            bit_idx += 1;
        }
    }

    hash_bytes
}

/// 批次計算單一檔案雜湊的結果型別
pub type HashItemResult = (PathBuf, Result<(String, Vec<u8>), DuplicateError>);

/// 批次平行計算多個檔案的 MD5 與 pHash (使用 Rayon 多執行緒)
pub fn batch_compute_hashes<P: AsRef<Path> + Sync>(
    paths: &[P],
    hash_size: u32,
) -> Vec<HashItemResult> {
    paths
        .par_iter()
        .map(|p| {
            let path_buf = p.as_ref().to_path_buf();
            let res = (|| {
                let md5 = compute_md5(&path_buf)?;
                let phash = compute_phash(&path_buf, hash_size)?;
                Ok((md5, phash))
            })();
            (path_buf, res)
        })
        .collect()
}

/// 計算 2D 離散餘弦變換 (DCT-II)
fn dct_2d(input: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    // 1. 行 DCT
    let mut row_dct = vec![0.0f64; rows * cols];
    for r in 0..rows {
        let row_start = r * cols;
        let row_data = &input[row_start..row_start + cols];
        let dct_row = dct_1d(row_data);
        row_dct[row_start..row_start + cols].copy_from_slice(&dct_row);
    }

    // 2. 列 DCT
    let mut result = vec![0.0f64; rows * cols];
    for c in 0..cols {
        let col_data: Vec<f64> = (0..rows).map(|r| row_dct[r * cols + c]).collect();
        let dct_col = dct_1d(&col_data);
        for r in 0..rows {
            result[r * cols + c] = dct_col[r];
        }
    }

    result
}

/// 計算 1D 離散餘弦變換 (DCT-II)
fn dct_1d(input: &[f64]) -> Vec<f64> {
    let n = input.len();
    let mut output = vec![0.0f64; n];
    let n_f64 = n as f64;

    for (k, out) in output.iter_mut().enumerate() {
        let mut sum = 0.0f64;
        for (i, &inp) in input.iter().enumerate() {
            sum += inp
                * ((std::f64::consts::PI * (2.0 * i as f64 + 1.0) * k as f64)
                    / (2.0 * n_f64))
                    .cos();
        }
        *out = sum;
    }

    output
}

/// 計算浮點數陣列之中位數
fn compute_median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let len = sorted.len();
    if len % 2 == 1 {
        sorted[len / 2]
    } else {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    }
}
