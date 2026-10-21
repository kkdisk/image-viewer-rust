//! core-archive: 壓縮檔安全讀取與受管理暫存生命週期

pub mod archive;
pub mod error;

pub use archive::{
    is_image_extension, sanitize_archive_path, ManagedArchiveSession,
    SUPPORTED_IMAGE_EXTENSIONS,
};
pub use error::ArchiveError;