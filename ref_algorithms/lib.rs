/// Image Viewer Rust 擴充模組
///
/// 提供影像處理加速功能，透過 PyO3 暴露給 Python 呼叫。
/// 所有函式設計為 graceful fallback — Python 端會在 import 失敗時
/// 自動退回純 Python 實作。

mod hashing;
mod comparison;
mod image_ops;

use pyo3::prelude::*;

/// Python 模組進入點
#[pymodule]
fn image_viewer_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Phase 1: 影像效果加速
    m.add_function(wrap_pyfunction!(image_ops::apply_white_balance, m)?)?;
    m.add_function(wrap_pyfunction!(image_ops::apply_fine_tune, m)?)?;

    // Phase 2: 雜湊計算
    m.add_function(wrap_pyfunction!(hashing::batch_md5, m)?)?;
    m.add_function(wrap_pyfunction!(hashing::batch_phash, m)?)?;
    m.add_function(wrap_pyfunction!(hashing::compute_single_md5, m)?)?;
    m.add_function(wrap_pyfunction!(hashing::compute_single_phash, m)?)?;

    // Phase 2: 相似度比對
    m.add_function(wrap_pyfunction!(comparison::find_similar_groups, m)?)?;
    m.add_function(wrap_pyfunction!(comparison::hamming_distance, m)?)?;

    Ok(())
}
