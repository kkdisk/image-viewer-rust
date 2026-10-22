//! core-archive: 壓縮檔安全讀取與受管理暫存生命週期

pub mod archive;
pub mod error;

pub use archive::{
    ArchiveLimits, ArchiveProgress, ManagedArchiveSession, ProgressCallback,
    SUPPORTED_IMAGE_EXTENSIONS, extract_7z_pure_rust, find_native_7z, is_image_extension,
    natural_cmp, sanitize_archive_path,
};
pub use error::ArchiveError;

pub mod recycle;
