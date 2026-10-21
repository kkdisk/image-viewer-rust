//! 壓縮檔安全讀取與暫存 Session 管理模組
//! 支援 ZIP 與 7Z 格式，嚴格防範 Path Traversal 漏洞，將圖片安全解壓至受管理之暫存目錄

use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Component, Path, PathBuf};
use tempfile::TempDir;

use crate::error::ArchiveError;

/// 支援的圖片副檔名清單
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "bmp", "webp", "gif", "tiff", "tif",
];

/// 判斷檔案是否為支援的圖片格式
pub fn is_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SUPPORTED_IMAGE_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// 安全路徑檢查：防範路徑穿透攻擊（Path Traversal）
/// 禁止含有 ParentDir ('..')、RootDir、Prefix 等跳出暫存目錄之惡意路徑
pub fn sanitize_archive_path(entry_name: &str) -> Result<PathBuf, ArchiveError> {
    let raw_path = Path::new(entry_name);

    // 檢查每一個路徑元件，僅允許 Normal 元件
    for comp in raw_path.components() {
        match comp {
            Component::Normal(_) => {}
            _ => {
                return Err(ArchiveError::PathTraversal(format!(
                    "檢測到潛在惡意路徑元件: {:?}",
                    entry_name
                )));
            }
        }
    }

    Ok(raw_path.to_path_buf())
}

/// 受管理的壓縮檔瀏覽 Session
/// 其生命週期持有 TempDir，當 Session 被釋放時自動清理暫存磁碟空間
pub struct ManagedArchiveSession {
    /// 原始壓縮檔路徑
    pub archive_path: PathBuf,
    /// 受管理的暫存目錄
    pub temp_dir: TempDir,
    /// 解壓出之所有有效圖片檔案路徑清單
    pub image_paths: Vec<PathBuf>,
}

impl ManagedArchiveSession {
    /// 開啟並安全解壓縮檔案至獨立暫存目錄（預設無密碼）
    pub fn open<P: AsRef<Path>>(archive_path: P) -> Result<Self, ArchiveError> {
        Self::open_with_password(archive_path, None)
    }

    /// 支援密碼保護之安全解壓縮流程
    pub fn open_with_password<P: AsRef<Path>>(
        archive_path: P,
        password: Option<&str>,
    ) -> Result<Self, ArchiveError> {
        let path = archive_path.as_ref();
        if !path.exists() {
            return Err(ArchiveError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("壓縮檔不存在: {}", path.display()),
            )));
        }

        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let temp_dir = tempfile::Builder::new()
            .prefix("rust_img_viewer_archive_")
            .tempdir()?;

        let mut image_paths = Vec::new();

        match ext.as_str() {
            "zip" => {
                extract_zip(path, temp_dir.path(), password, &mut image_paths)?;
            }
            "7z" => {
                extract_7z(path, temp_dir.path(), password, &mut image_paths)?;
            }
            _ => {
                return Err(ArchiveError::UnsupportedFormat(format!(
                    "未知的壓縮檔副檔名: .{}",
                    ext
                )));
            }
        }

        if image_paths.is_empty() {
            return Err(ArchiveError::NoImagesFound);
        }

        // 依檔名自然排序
        image_paths.sort();

        Ok(Self {
            archive_path: path.to_path_buf(),
            temp_dir,
            image_paths,
        })
    }

    /// 檢查給定路徑是否來自此壓縮包暫存目錄
    pub fn is_from_archive<P: AsRef<Path>>(&self, path: P) -> bool {
        path.as_ref().starts_with(self.temp_dir.path())
    }
}

/// 安全解壓 ZIP 檔案（支援密碼）
fn extract_zip(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    image_paths: &mut Vec<PathBuf>,
) -> Result<(), ArchiveError> {
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|e| ArchiveError::Zip(e.to_string()))?;

    for i in 0..zip.len() {
        let mut file = match password {
            Some(pwd) if !pwd.is_empty() => match zip.by_index_decrypt(i, pwd.as_bytes()) {
                Ok(f) => f,
                Err(zip::result::ZipError::InvalidPassword) => {
                    return Err(ArchiveError::InvalidPassword);
                }
                Err(e) => return Err(ArchiveError::Zip(e.to_string())),
            },
            _ => {
                let f = zip
                    .by_index(i)
                    .map_err(|e| ArchiveError::Zip(e.to_string()))?;
                if f.encrypted() {
                    return Err(ArchiveError::PasswordRequired);
                }
                f
            }
        };

        // 優先使用 enclosed_name 檢查路徑安全
        let rel_path = match file.enclosed_name() {
            Some(path) => sanitize_archive_path(&path.to_string_lossy())?,
            None => {
                return Err(ArchiveError::PathTraversal(format!(
                    "無效或危險的檔案名稱: {}",
                    file.name()
                )));
            }
        };

        let target_path = dest_dir.join(&rel_path);

        if file.is_dir() {
            fs::create_dir_all(&target_path)?;
        } else {
            // 僅處理支援的圖片格式
            if is_image_extension(&rel_path) {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out_file = File::create(&target_path)?;
                io::copy(&mut file, &mut out_file)?;
                image_paths.push(target_path);
            }
        }
    }

    Ok(())
}

/// 安全解壓 7Z 檔案（支援密碼）
fn extract_7z(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    image_paths: &mut Vec<PathBuf>,
) -> Result<(), ArchiveError> {
    let pwd = match password {
        Some(p) if !p.is_empty() => sevenz_rust::Password::from(p),
        _ => sevenz_rust::Password::empty(),
    };

    let mut reader = sevenz_rust::SevenZReader::open(archive_path, pwd)
        .map_err(ArchiveError::from_sevenz)?;

    reader
        .for_each_entries(|entry, read| {
            let rel_path = sanitize_archive_path(entry.name())
                .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))?;

            if entry.is_directory() {
                let dir_path = dest_dir.join(&rel_path);
                fs::create_dir_all(&dir_path)?;
                return Ok(true);
            }

            if is_image_extension(&rel_path) {
                let file_path = dest_dir.join(&rel_path);
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut out_file = File::create(&file_path)?;
                io::copy(read, &mut out_file)?;
                image_paths.push(file_path);
            }

            Ok(true)
        })
        .map_err(ArchiveError::from_sevenz)?;

    Ok(())
}
