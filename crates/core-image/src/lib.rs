//! 核心影像處理與像素運算庫 (core-image)
//!
//! 提供影像編解碼、EXIF Orientation 解析與方向校正、像素管線微調，以及安全保護與原子儲存。

pub mod curves;
pub mod display_preview;
pub mod error;
pub mod exif;
pub mod geometry;
pub mod io;
pub mod ops;
pub mod output;

pub use error::ImageError;
pub use exif::{
    apply_orientation, read_orientation, read_orientation_from_bytes, read_orientation_from_file,
};
pub use io::{
    MAX_SAFE_PIXELS, check_pixel_limit, inspect_dimensions, inspect_dimensions_from_memory,
    load_image, load_image_from_memory, load_image_from_memory_with_limit, load_image_with_limit,
    save_image_atomic,
};
pub use ops::{
    AdjustmentParams, HistogramData, apply_adjustments, apply_adjustments_rgba,
    apply_fine_tune_raw, apply_white_balance_raw,
};

/// 取得 core-image 模組版本
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
