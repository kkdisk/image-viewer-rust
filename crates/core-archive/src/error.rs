//! 壓縮檔處理強型別錯誤模組

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error("I/O 錯誤: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP 格式解析錯誤: {0}")]
    Zip(String),

    #[error("7Z 格式解析錯誤: {0}")]
    SevenZ(String),

    #[error("路徑安全性檢查失敗 (Path Traversal 防護): {0}")]
    PathTraversal(String),

    #[error("壓縮檔中未包含任何支援的圖片檔案")]
    NoImagesFound,

    #[error("不支援的壓縮格式: {0}")]
    UnsupportedFormat(String),
}
