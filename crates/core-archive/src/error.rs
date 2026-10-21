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

    #[error("需要密碼才能解密此壓縮檔")]
    PasswordRequired,

    #[error("密碼錯誤或解密失敗")]
    InvalidPassword,

    #[error("路徑安全性檢查失敗 (Path Traversal 防護): {0}")]
    PathTraversal(String),

    #[error("壓縮檔中未包含任何支援的圖片檔案")]
    NoImagesFound,

    #[error("不支援的壓縮格式: {0}")]
    UnsupportedFormat(String),
}

impl ArchiveError {
    /// 將 sevenz-rust 的 Error 轉換為強型別之 ArchiveError
    pub fn from_sevenz(err: sevenz_rust::Error) -> Self {
        match err {
            sevenz_rust::Error::PasswordRequired => ArchiveError::PasswordRequired,
            sevenz_rust::Error::MaybeBadPassword(_) => ArchiveError::InvalidPassword,
            sevenz_rust::Error::ChecksumVerificationFailed => ArchiveError::InvalidPassword,
            sevenz_rust::Error::NextHeaderCrcMismatch => ArchiveError::InvalidPassword,
            sevenz_rust::Error::Io(e, _) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("password") {
                    ArchiveError::InvalidPassword
                } else {
                    ArchiveError::Io(e)
                }
            }
            sevenz_rust::Error::FileOpen(e, _) => ArchiveError::Io(e),
            other => {
                let s = other.to_string();
                let lower = s.to_lowercase();
                if lower.contains("password") {
                    if lower.contains("required") || lower.contains("not provided") {
                        ArchiveError::PasswordRequired
                    } else {
                        ArchiveError::InvalidPassword
                    }
                } else {
                    ArchiveError::SevenZ(s)
                }
            }
        }
    }
}
