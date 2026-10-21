/// 雜湊計算模組 — MD5 檔案雜湊與感知雜湊 (pHash)
///
/// 支援單檔與批次平行計算。批次模式使用 Rayon 實現多執行緒 I/O 與計算。

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use md5::{Digest, Md5};
use pyo3::exceptions::PyIOError;
use pyo3::prelude::*;
use rayon::prelude::*;

// ============================================================================
// MD5 檔案雜湊
// ============================================================================

/// 計算單一檔案的 MD5 雜湊值。
///
/// 以 64KB 緩衝區串流讀取，不會一次載入整個檔案。
///
/// Args:
///     file_path: 檔案絕對路徑。
///
/// Returns:
///     32 字元的十六進位 MD5 字串。
///
/// Raises:
///     IOError: 檔案不存在或無法讀取。
#[pyfunction]
pub fn compute_single_md5(file_path: &str) -> PyResult<String> {
    compute_md5_internal(file_path)
        .map_err(|e| PyIOError::new_err(format!("MD5 計算失敗 ({}): {}", file_path, e)))
}

/// 批次計算多個檔案的 MD5 雜湊值（使用 Rayon 平行處理）。
///
/// Args:
///     file_paths: 檔案路徑清單。
///
/// Returns:
///     清單：每項為 (路徑, 結果) 元組。
///     成功時結果為 MD5 hex 字串；失敗時為 "ERROR:" 開頭的錯誤訊息。
#[pyfunction]
pub fn batch_md5(file_paths: Vec<String>) -> PyResult<Vec<(String, String)>> {
    let results: Vec<(String, String)> = file_paths
        .par_iter()
        .map(|path| {
            let result = match compute_md5_internal(path) {
                Ok(hash) => hash,
                Err(e) => format!("ERROR:{}", e),
            };
            (path.clone(), result)
        })
        .collect();
    Ok(results)
}

/// 內部 MD5 計算實作
fn compute_md5_internal(file_path: &str) -> Result<String, String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("檔案不存在: {}", file_path));
    }

    let file =
        File::open(path).map_err(|e| format!("無法開啟檔案 {}: {}", file_path, e))?;
    let mut reader = BufReader::with_capacity(65536, file);
    let mut hasher = Md5::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| format!("讀取檔案 {} 失敗: {}", file_path, e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

// ============================================================================
// 感知雜湊 (pHash)
// ============================================================================

/// 計算單一圖片的感知雜湊 (pHash)。
///
/// 流程：讀取圖片 → 縮放至 (hash_size*4) x (hash_size*4) → 灰階 →
///       計算 DCT → 取左上 hash_size x hash_size → 中位數二值化。
///
/// Args:
///     file_path: 圖片檔案路徑。
///     hash_size: 雜湊大小（預設 8，產生 64-bit hash）。
///
/// Returns:
///     雜湊值的 bytes（長度 = hash_size * hash_size / 8）。
///
/// Raises:
///     IOError: 圖片無法開啟或格式不支援。
#[pyfunction]
#[pyo3(signature = (file_path, hash_size=8))]
pub fn compute_single_phash(file_path: &str, hash_size: u32) -> PyResult<Vec<u8>> {
    compute_phash_internal(file_path, hash_size)
        .map_err(|e| PyIOError::new_err(format!("pHash 計算失敗 ({}): {}", file_path, e)))
}

/// 批次計算多個圖片的感知雜湊（使用 Rayon 平行處理）。
///
/// Args:
///     file_paths: 圖片路徑清單。
///     hash_size: 雜湊大小（預設 8）。
///
/// Returns:
///     清單：每項為 (路徑, 結果) 元組。
///     成功時結果為 hash bytes；失敗時為空 Vec。
#[pyfunction]
#[pyo3(signature = (file_paths, hash_size=8))]
pub fn batch_phash(
    file_paths: Vec<String>,
    hash_size: u32,
) -> PyResult<Vec<(String, Vec<u8>)>> {
    let results: Vec<(String, Vec<u8>)> = file_paths
        .par_iter()
        .map(|path| {
            let hash = compute_phash_internal(path, hash_size).unwrap_or_default();
            (path.clone(), hash)
        })
        .collect();
    Ok(results)
}

/// 內部 pHash 計算實作
///
/// 使用簡化的 DCT-based pHash 演算法：
/// 1. 將圖片縮放至 (hash_size*4) x (hash_size*4)
/// 2. 轉為灰階
/// 3. 計算 DCT（離散餘弦變換）
/// 4. 取左上 hash_size x hash_size 低頻區域
/// 5. 以中位數為閾值進行二值化
fn compute_phash_internal(file_path: &str, hash_size: u32) -> Result<Vec<u8>, String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(format!("檔案不存在: {}", file_path));
    }

    // 讀取圖片
    let img = image::open(path).map_err(|e| format!("無法開啟圖片 {}: {}", file_path, e))?;

    // 縮放至 (hash_size * 4) x (hash_size * 4)，使用 Lanczos3 重採樣
    let resize_dim = hash_size * 4;
    let resized = img.resize_exact(resize_dim, resize_dim, image::imageops::FilterType::Lanczos3);

    // 轉為灰階
    let gray = resized.to_luma8();

    let n = resize_dim as usize;

    // 將像素值轉為 f64 矩陣
    let pixels: Vec<f64> = gray.pixels().map(|p| p.0[0] as f64).collect();

    // 計算 2D DCT (Type-II)
    let dct = dct_2d(&pixels, n, n);

    // 取左上 hash_size x hash_size 的低頻分量（跳過 DC 分量 [0,0]）
    let hs = hash_size as usize;
    let mut low_freq: Vec<f64> = Vec::with_capacity(hs * hs - 1);
    for y in 0..hs {
        for x in 0..hs {
            if x == 0 && y == 0 {
                continue; // 跳過 DC 分量
            }
            low_freq.push(dct[y * n + x]);
        }
    }

    // 計算中位數
    let median = compute_median(&low_freq);

    // 二值化：大於中位數為 1，否則為 0
    // 將 hash_size * hash_size bits 打包成 bytes
    let total_bits = (hs * hs) as usize;
    let num_bytes = (total_bits + 7) / 8;
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

    Ok(hash_bytes)
}

/// 計算 2D DCT (Type-II)
///
/// 先對每一行做 1D DCT，再對每一列做 1D DCT。
fn dct_2d(input: &[f64], rows: usize, cols: usize) -> Vec<f64> {
    // 先對行做 DCT
    let mut row_dct = vec![0.0f64; rows * cols];
    for r in 0..rows {
        let row_start = r * cols;
        let row_data = &input[row_start..row_start + cols];
        let dct_row = dct_1d(row_data);
        row_dct[row_start..row_start + cols].copy_from_slice(&dct_row);
    }

    // 再對列做 DCT
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

/// 計算 1D DCT (Type-II)
fn dct_1d(input: &[f64]) -> Vec<f64> {
    let n = input.len();
    let mut output = vec![0.0f64; n];
    let n_f64 = n as f64;

    for k in 0..n {
        let mut sum = 0.0f64;
        for i in 0..n {
            sum += input[i]
                * ((std::f64::consts::PI * (2.0 * i as f64 + 1.0) * k as f64)
                    / (2.0 * n_f64))
                    .cos();
        }
        output[k] = sum;
    }

    output
}

/// 計算中位數
fn compute_median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dct_1d_basic() {
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = dct_1d(&input);
        assert_eq!(output.len(), 4);
        // DC 分量應等於所有值的總和
        assert!((output[0] - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_median_odd() {
        let values = vec![3.0, 1.0, 2.0];
        assert!((compute_median(&values) - 2.0).abs() < 1e-10);
    }

    #[test]
    fn test_compute_median_even() {
        let values = vec![1.0, 2.0, 3.0, 4.0];
        assert!((compute_median(&values) - 2.5).abs() < 1e-10);
    }
}
