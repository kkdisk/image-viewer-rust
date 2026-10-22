//! 核心影像處理錯誤型別

use thiserror::Error;

/// 影像處理核心錯誤
#[derive(Debug, Error)]
pub enum ImageError {
    /// 檔案 I/O 錯誤
    #[error("檔案 I/O 錯誤: {0}")]
    Io(#[from] std::io::Error),

    /// 底層 image crate 解碼或編碼錯誤
    #[error("影像解碼/編碼錯誤: {0}")]
    Image(#[from] image::ImageError),

    /// EXIF 元資料讀取或解析錯誤
    #[error("EXIF 解析錯誤: {0}")]
    Exif(String),

    /// 影像總像素數超過安全上限（防止記憶體解壓炸彈）
    #[error(
        "影像尺寸過大 ({width}x{height} = {total_pixels} 像素)，超過安全上限 {max_pixels} 像素"
    )]
    ImageTooLarge {
        /// 圖片寬度
        width: u32,
        /// 圖片高度
        height: u32,
        /// 總像素數
        total_pixels: u64,
        /// 允許的安全上限像素數
        max_pixels: u64,
    },

    /// 不支援或無法辨識的影像格式
    #[error("不支援或無法辨識的影像格式: {0}")]
    UnsupportedFormat(String),
}
