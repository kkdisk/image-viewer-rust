//! duplicate-finder 強型別錯誤定義

use std::path::PathBuf;
use thiserror::Error;

/// 重複圖片查重錯誤列舉
#[derive(Error, Debug)]
pub enum DuplicateError {
    #[error("檔案不存在或無法存取: {path:?}")]
    IoError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("影像解碼失敗: {path:?}, 理由: {message}")]
    DecodeError {
        path: PathBuf,
        message: String,
    },

    #[error("雜湊計算失敗: {0}")]
    HashError(String),
}
