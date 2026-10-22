//! 壓縮檔安全讀取與暫存 Session 管理模組
//! 支援 ZIP 與 7Z 格式，嚴格防範 Path Traversal 與 Zip-bomb 漏洞，將圖片安全解壓至受管理之暫存目錄

use std::cmp::Ordering;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

use crate::error::ArchiveError;

/// 解壓縮即時進度資訊
#[derive(Debug, Clone, Default)]
pub struct ArchiveProgress {
    /// 當前正在處理的條目檔名
    pub current_entry: String,
    /// 目前已處理條目序號 (從 1 起算)
    pub current_entry_index: usize,
    /// 預估或已發現之條目總數 (若未知則為 0)
    pub total_entries: usize,
    /// 目前已解壓/寫入之總位元組數
    pub bytes_written: u64,
}

pub type ProgressCallback = Arc<dyn Fn(ArchiveProgress) + Send + Sync>;

/// 尋找系統中可用的原生 7-Zip (7z.exe) 執行檔（提供 50 倍以上的高效解壓）
pub fn find_native_7z() -> Option<PathBuf> {
    // 1. 與當前應用程式同目錄下的 7z.exe
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(parent) = exe_path.parent()
    {
        let local_7z = parent.join("7z.exe");
        if local_7z.is_file() {
            return Some(local_7z);
        }
    }
    // 2. 64-bit 預設安裝路徑
    let p64 = PathBuf::from(r"C:\Program Files\7-Zip\7z.exe");
    if p64.is_file() {
        return Some(p64);
    }
    // 3. 32-bit 預設安裝路徑
    let p86 = PathBuf::from(r"C:\Program Files (x86)\7-Zip\7z.exe");
    if p86.is_file() {
        return Some(p86);
    }
    // 4. PATH 環境變數中的 7z.exe
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join("7z.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 支援的圖片副檔名清單
pub const SUPPORTED_IMAGE_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "bmp", "webp", "gif", "tiff", "tif"];

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

/// 自然檔名排序比較演算法（例如 "img2.jpg" 排在 "img10.jpg" 之前）
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut chars_a = a.chars().peekable();
    let mut chars_b = b.chars().peekable();

    while let (Some(&ca), Some(&cb)) = (chars_a.peek(), chars_b.peek()) {
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let mut num_a: u64 = 0;
            while let Some(&c) = chars_a.peek() {
                if let Some(digit) = c.to_digit(10) {
                    num_a = num_a.saturating_mul(10).saturating_add(digit as u64);
                    chars_a.next();
                } else {
                    break;
                }
            }

            let mut num_b: u64 = 0;
            while let Some(&c) = chars_b.peek() {
                if let Some(digit) = c.to_digit(10) {
                    num_b = num_b.saturating_mul(10).saturating_add(digit as u64);
                    chars_b.next();
                } else {
                    break;
                }
            }

            match num_a.cmp(&num_b) {
                Ordering::Equal => {}
                non_eq => return non_eq,
            }
        } else {
            let lower_a = ca.to_ascii_lowercase();
            let lower_b = cb.to_ascii_lowercase();
            match lower_a.cmp(&lower_b) {
                Ordering::Equal => {
                    chars_a.next();
                    chars_b.next();
                }
                non_eq => return non_eq,
            }
        }
    }

    chars_a.peek().is_some().cmp(&chars_b.peek().is_some())
}

/// 解壓安全限制配置 (防範 Zip-bomb 資源耗盡攻擊)
#[derive(Debug, Clone)]
pub struct ArchiveLimits {
    /// 壓縮檔內最大允許條目數
    pub max_entries: usize,
    /// 單一檔案最大允許解壓大小 (位元組)
    pub max_entry_bytes: u64,
    /// 整個壓縮檔累計最大允許解壓大小 (位元組)
    pub max_total_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 10_000,
            max_entry_bytes: 256 * 1024 * 1024,  // 256 MB
            max_total_bytes: 1024 * 1024 * 1024, // 1 GB
        }
    }
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

/// 安全受限串流複製：防範單檔或總解壓量超出上限
fn copy_with_limits<R: Read + ?Sized, W: Write>(
    reader: &mut R,
    writer: &mut W,
    max_entry_bytes: u64,
    total_bytes_written: &mut u64,
    max_total_bytes: u64,
    entry_name: &str,
) -> Result<u64, ArchiveError> {
    let mut buffer = [0u8; 65536];
    let mut entry_bytes_read = 0u64;

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }

        entry_bytes_read = entry_bytes_read.saturating_add(n as u64);
        if entry_bytes_read > max_entry_bytes {
            return Err(ArchiveError::LimitExceeded(format!(
                "條目 '{}' 解壓大小超過單檔上限 ({} 位元組)",
                entry_name, max_entry_bytes
            )));
        }

        *total_bytes_written = total_bytes_written.saturating_add(n as u64);
        if *total_bytes_written > max_total_bytes {
            return Err(ArchiveError::LimitExceeded(format!(
                "壓縮檔總解壓大小超過上限 ({} 位元組)",
                max_total_bytes
            )));
        }

        writer.write_all(&buffer[..n])?;
    }

    Ok(entry_bytes_read)
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
    /// 開啟並安全解壓縮檔案至獨立暫存目錄（預設無密碼與預設安全限制）
    pub fn open<P: AsRef<Path>>(archive_path: P) -> Result<Self, ArchiveError> {
        Self::open_with_password(archive_path, None)
    }

    /// 支援密碼保護之安全解壓縮流程（預設安全限制）
    pub fn open_with_password<P: AsRef<Path>>(
        archive_path: P,
        password: Option<&str>,
    ) -> Result<Self, ArchiveError> {
        Self::open_with_password_and_limits(archive_path, password, &ArchiveLimits::default())
    }

    /// 支援自訂安全限制與密碼之解壓縮流程
    pub fn open_with_password_and_limits<P: AsRef<Path>>(
        archive_path: P,
        password: Option<&str>,
        limits: &ArchiveLimits,
    ) -> Result<Self, ArchiveError> {
        Self::open_with_progress_and_limits(archive_path, password, limits, None)
    }

    /// 完整支援即時進度回呼、自訂安全限制與密碼之解壓縮流程
    pub fn open_with_progress_and_limits<P: AsRef<Path>>(
        archive_path: P,
        password: Option<&str>,
        limits: &ArchiveLimits,
        progress: Option<ProgressCallback>,
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
                extract_zip(
                    path,
                    temp_dir.path(),
                    password,
                    limits,
                    &mut image_paths,
                    progress.as_ref(),
                )?;
            }
            "7z" => {
                extract_7z(
                    path,
                    temp_dir.path(),
                    password,
                    limits,
                    &mut image_paths,
                    progress.as_ref(),
                )?;
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

        // 依檔名自然排序（自然數字排序）
        image_paths.sort_by(|a, b| {
            let name_a = a.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let name_b = b.file_name().and_then(|n| n.to_str()).unwrap_or("");
            natural_cmp(name_a, name_b)
        });

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

/// 安全解壓 ZIP 檔案（支援密碼、Zip-bomb 安全限制與即時進度回呼）
fn extract_zip(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    limits: &ArchiveLimits,
    image_paths: &mut Vec<PathBuf>,
    progress: Option<&ProgressCallback>,
) -> Result<(), ArchiveError> {
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| ArchiveError::Zip(e.to_string()))?;

    let total_entries = zip.len();
    if total_entries > limits.max_entries {
        return Err(ArchiveError::LimitExceeded(format!(
            "壓縮檔條目數 ({}) 超出安全上限 ({})",
            total_entries, limits.max_entries
        )));
    }

    let mut total_bytes_written = 0u64;

    for i in 0..total_entries {
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

        let entry_name_str = file.name().to_string();
        if let Some(cb) = progress {
            cb(ArchiveProgress {
                current_entry: entry_name_str.clone(),
                current_entry_index: i + 1,
                total_entries,
                bytes_written: total_bytes_written,
            });
        }

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
                let entry_name = rel_path.to_string_lossy().to_string();
                let mut out_file = File::create(&target_path)?;
                copy_with_limits(
                    &mut file,
                    &mut out_file,
                    limits.max_entry_bytes,
                    &mut total_bytes_written,
                    limits.max_total_bytes,
                    &entry_name,
                )?;
                image_paths.push(target_path);
            }
        }
    }

    Ok(())
}

/// 純 Rust 安全解壓 7Z 檔案（保證 100% 裸機零相依獨立執行，支援即時進度回呼與安全限制）
pub fn extract_7z_pure_rust(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    limits: &ArchiveLimits,
    image_paths: &mut Vec<PathBuf>,
    progress: Option<&ProgressCallback>,
) -> Result<(), ArchiveError> {
    let has_password = password.map(|p| !p.is_empty()).unwrap_or(false);
    let pwd = password
        .filter(|p| !p.is_empty())
        .map(sevenz_rust::Password::from)
        .unwrap_or_else(sevenz_rust::Password::empty);

    let mut reader = sevenz_rust::SevenZReader::open(archive_path, pwd)
        .map_err(|e| ArchiveError::from_sevenz(e, has_password))?;

    validate_7z_header(reader.archive(), limits)?;
    let total_entries = reader.archive().files.len();
    let mut total_bytes_written = 0u64;
    let mut entry_count = 0usize;
    let mut explicit_error: Option<ArchiveError> = None;

    let res = reader.for_each_entries(|entry, read| {
        entry_count = entry_count.saturating_add(1);
        if entry_count > limits.max_entries {
            explicit_error = Some(ArchiveError::LimitExceeded(format!(
                "7Z 壓縮檔條目數超過安全上限 ({})",
                limits.max_entries
            )));
            return Err(sevenz_rust::Error::other("條目數超限"));
        }

        let entry_name_str = entry.name().to_string();
        if let Some(cb) = progress {
            cb(ArchiveProgress {
                current_entry: entry_name_str.clone(),
                current_entry_index: entry_count,
                total_entries,
                bytes_written: total_bytes_written,
            });
        }

        let rel_path = match sanitize_archive_path(entry.name()) {
            Ok(p) => p,
            Err(e) => {
                explicit_error = Some(e);
                return Err(sevenz_rust::Error::other("Path traversal"));
            }
        };

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
            if let Err(e) = copy_with_limits(
                read,
                &mut out_file,
                limits.max_entry_bytes,
                &mut total_bytes_written,
                limits.max_total_bytes,
                entry.name(),
            ) {
                explicit_error = Some(e);
                return Err(sevenz_rust::Error::other("解壓超出限制"));
            }
            image_paths.push(file_path);
        } else {
            // 固實壓縮 (Solid Archive) 必須完整消費未使用的 entry 資料流
            // 同時加上限制防範未使用的 entry 成為惡意巨量資料流
            if let Err(e) = copy_with_limits(
                read,
                &mut io::sink(),
                limits.max_entry_bytes,
                &mut total_bytes_written,
                limits.max_total_bytes,
                entry.name(),
            ) {
                explicit_error = Some(e);
                return Err(sevenz_rust::Error::other("非圖片條目解壓超限"));
            }
        }

        Ok(true)
    });

    if let Err(e) = res {
        if let Some(err) = explicit_error {
            return Err(err);
        }
        return Err(ArchiveError::from_sevenz(e, has_password));
    }

    Ok(())
}

fn validate_7z_header(
    archive: &sevenz_rust::Archive,
    limits: &ArchiveLimits,
) -> Result<(), ArchiveError> {
    if archive.files.len() > limits.max_entries {
        return Err(ArchiveError::LimitExceeded(
            "7Z 壓縮檔條目數超過安全上限".into(),
        ));
    }
    let mut declared_total = 0u64;
    for entry in &archive.files {
        sanitize_archive_path(entry.name())?;
        declared_total = declared_total
            .checked_add(entry.size)
            .ok_or_else(|| ArchiveError::LimitExceeded("7Z 解壓容量總和溢位".into()))?;
        if entry.size > limits.max_entry_bytes || declared_total > limits.max_total_bytes {
            return Err(ArchiveError::LimitExceeded(
                "7Z 宣告容量超過解壓安全上限".into(),
            ));
        }
    }
    Ok(())
}

/// 任何錯誤或提前返回都會結束子程序並回收，避免管線阻塞或殘留程序。
struct NativeProcess(std::process::Child);

impl Drop for NativeProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct CheckedStream<R> {
    reader: R,
    checksum: crc32fast::Hasher,
}

impl<R: Read> Read for CheckedStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(buf)?;
        self.checksum.update(&buf[..count]);
        Ok(count)
    }
}

/// 原生引擎只輸出資料串流；Rust 控制所有路徑、寫入與容量限制。
fn extract_native_stream(
    executable: &Path,
    archive_path: &Path,
    password: Option<&str>,
    archive: &sevenz_rust::Archive,
    dest_dir: &Path,
    progress: Option<&ProgressCallback>,
) -> Result<Option<Vec<PathBuf>>, ArchiveError> {
    use std::process::{Command, Stdio};
    // 每個資料流都必須能驗證 CRC，避免不同版本的輸出順序造成內容錯配。
    if archive
        .files
        .iter()
        .any(|e| e.is_anti_item || (e.has_stream && !e.has_crc))
    {
        return Ok(None);
    }
    let mut cmd = Command::new(executable);
    cmd.args(["x", "-so", "-y", "-bso0", "-bsp0"])
        .arg(format!(
            "-p{}",
            password.filter(|p| !p.is_empty()).unwrap_or("-")
        ))
        .arg("--")
        .arg(archive_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = match cmd.spawn() {
        Ok(child) => NativeProcess(child),
        Err(_) => return Ok(None),
    };
    let Some(mut stdout) = child.0.stdout.take() else {
        return Ok(None);
    };
    let staging = tempfile::Builder::new()
        .prefix("native_")
        .tempdir_in(dest_dir)?;
    let mut image_paths = Vec::new();
    let mut total = 0u64;
    for (index, entry) in archive.files.iter().enumerate() {
        let relative = sanitize_archive_path(entry.name())?;
        let mut stream = CheckedStream {
            reader: (&mut stdout).take(entry.size),
            checksum: crc32fast::Hasher::new(),
        };
        let mut written = 0u64;
        let image = !entry.is_directory && is_image_extension(&relative);
        let path = staging.path().join(relative);
        let mut destination: Box<dyn Write> = if image {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            Box::new(File::create(&path)?)
        } else {
            Box::new(io::sink())
        };
        // 標頭已通過總量檢查；逐條目限制實際讀取，且不允許額外資料落盤。
        let copied = copy_with_limits(
            &mut stream,
            &mut destination,
            entry.size,
            &mut written,
            entry.size,
            entry.name(),
        )?;
        if copied != entry.size
            || (entry.has_stream && stream.checksum.finalize() != entry.crc as u32)
        {
            return Ok(None);
        }
        drop(destination);
        total += copied;
        if image {
            image_paths.push(path);
        }
        if let Some(cb) = progress {
            cb(ArchiveProgress {
                current_entry: entry.name.clone(),
                current_entry_index: index + 1,
                total_entries: archive.files.len(),
                bytes_written: total,
            });
        }
    }
    let mut extra = [0u8; 1];
    if stdout.read(&mut extra)? != 0 {
        return Err(ArchiveError::LimitExceeded(
            "原生解壓串流超過宣告容量".into(),
        ));
    }
    if !child.0.wait()?.success() {
        // 退出碼 2 包含多種錯誤，交由純 Rust 引擎重新分類，不能直接當成密碼錯誤。
        return Ok(None);
    }
    let _ = staging.keep();
    Ok(Some(image_paths))
}

fn extract_7z(
    archive_path: &Path,
    dest_dir: &Path,
    password: Option<&str>,
    limits: &ArchiveLimits,
    image_paths: &mut Vec<PathBuf>,
    progress: Option<&ProgressCallback>,
) -> Result<(), ArchiveError> {
    let pwd = password
        .filter(|p| !p.is_empty())
        .map(sevenz_rust::Password::from)
        .unwrap_or_else(sevenz_rust::Password::empty);
    let reader = sevenz_rust::SevenZReader::open(archive_path, pwd)
        .map_err(|e| ArchiveError::from_sevenz(e, password.is_some_and(|p| !p.is_empty())))?;
    validate_7z_header(reader.archive(), limits)?;
    if let Some(executable) = find_native_7z()
        && let Some(paths) = extract_native_stream(
            &executable,
            archive_path,
            password,
            reader.archive(),
            dest_dir,
            progress,
        )?
    {
        image_paths.extend(paths);
        return Ok(());
    }
    extract_7z_pure_rust(
        archive_path,
        dest_dir,
        password,
        limits,
        image_paths,
        progress,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "需原生 7-Zip 的發布模式效能量測"]
    fn native_stream_performance() {
        let executable = find_native_7z().expect("效能量測需要原生 7-Zip");
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let mut seed = 42u32;
        let block: Vec<u8> = (0..262144)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                seed as u8
            })
            .collect();
        for index in 0..8 {
            let mut file = File::create(source.join(format!("{index}.png"))).unwrap();
            for _ in 0..16 {
                file.write_all(&block).unwrap();
            }
        }
        let path = root.path().join("benchmark.7z");
        assert!(
            std::process::Command::new(&executable)
                .current_dir(&source)
                .args(["a", "-bso0", "-ms=on", "-mx=1"])
                .arg(&path)
                .arg(".")
                .status()
                .unwrap()
                .success()
        );
        let direct_dest = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        assert!(
            std::process::Command::new(&executable)
                .args(["x", "-bso0", "-y"])
                .arg(format!("-o{}", direct_dest.path().display()))
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let direct_time = started.elapsed();
        let reader =
            sevenz_rust::SevenZReader::open(&path, sevenz_rust::Password::empty()).unwrap();
        let native_dest = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        let native = extract_native_stream(
            &executable,
            &path,
            None,
            reader.archive(),
            native_dest.path(),
            None,
        )
        .unwrap()
        .unwrap();
        let native_time = started.elapsed();
        let pure_dest = tempfile::tempdir().unwrap();
        let mut pure = Vec::new();
        let started = std::time::Instant::now();
        extract_7z_pure_rust(
            &path,
            pure_dest.path(),
            None,
            &ArchiveLimits::default(),
            &mut pure,
            None,
        )
        .unwrap();
        let pure_time = started.elapsed();
        assert_eq!(native.len(), pure.len());
        for path in &native {
            assert_eq!(
                fs::read(path).unwrap(),
                fs::read(source.join(path.file_name().unwrap())).unwrap()
            );
        }
        println!(
            "32 MiB 固實測資：原生直接寫檔 {direct_time:?}，原生受限串流 {native_time:?}，純 Rust {pure_time:?}"
        );
    }

    #[test]
    fn native_stream_preserves_solid_and_non_solid_contents() {
        let Some(executable) = find_native_7z() else {
            return;
        };
        for solid in ["on", "off"] {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            fs::create_dir_all(source.join("nested/empty")).unwrap();
            let files = [
                ("a.png", vec![1u8; 111]),
                ("b.txt", vec![2u8; 333]),
                ("nested/c.png", vec![3u8; 777]),
                ("nested/d.jpg", vec![4u8; 93]),
                ("empty.png", Vec::new()),
            ];
            for (name, bytes) in &files {
                fs::write(source.join(name), bytes).unwrap();
            }
            let path = root.path().join("gallery.7z");
            assert!(
                std::process::Command::new(&executable)
                    .current_dir(&source)
                    .args(["a", "-bso0", &format!("-ms={solid}")])
                    .arg(&path)
                    .arg(".")
                    .status()
                    .unwrap()
                    .success()
            );
            let reader =
                sevenz_rust::SevenZReader::open(&path, sevenz_rust::Password::empty()).unwrap();
            let dest = tempfile::tempdir().unwrap();
            validate_7z_header(reader.archive(), &ArchiveLimits::default()).unwrap();
            let paths = extract_native_stream(
                &executable,
                &path,
                None,
                reader.archive(),
                dest.path(),
                None,
            )
            .unwrap()
            .expect("必須走原生串流，不能靜默降級");
            assert_eq!(paths.len(), 4);
            for output in &paths {
                let name = output.file_name().unwrap();
                let (_, expected) = files
                    .iter()
                    .find(|(p, _)| Path::new(p).file_name() == Some(name))
                    .unwrap();
                assert_eq!(&fs::read(output).unwrap(), expected);
            }
            // CRC 不符必須捨棄原生結果，暫存檔也必須清除。
            let mut wrong_header = reader.archive().clone();
            let entry = wrong_header
                .files
                .iter_mut()
                .find(|e| e.has_stream)
                .unwrap();
            entry.crc ^= 1;
            let failed_dest = tempfile::tempdir().unwrap();
            assert!(
                extract_native_stream(
                    &executable,
                    &path,
                    None,
                    &wrong_header,
                    failed_dest.path(),
                    None
                )
                .unwrap()
                .is_none()
            );
            assert_eq!(fs::read_dir(failed_dest.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn sevenz_limits_reject_before_writing_with_both_engines() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("photo.png");
        fs::write(&source, vec![0u8; 1024]).unwrap();
        let path = root.path().join("gallery.7z");
        sevenz_rust::compress_to_path(&source, &path).unwrap();
        let limits = ArchiveLimits {
            max_entry_bytes: 10,
            ..ArchiveLimits::default()
        };
        for pure in [false, true] {
            let dest = tempfile::tempdir().unwrap();
            let mut paths = Vec::new();
            let result = if pure {
                extract_7z_pure_rust(&path, dest.path(), None, &limits, &mut paths, None)
            } else {
                extract_7z(&path, dest.path(), None, &limits, &mut paths, None)
            };
            assert!(matches!(result, Err(ArchiveError::LimitExceeded(_))));
            assert!(paths.is_empty());
            assert_eq!(fs::read_dir(dest.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn corrupt_archive_is_not_a_password_request() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("corrupt.7z");
        fs::write(&path, b"invalid archive").unwrap();
        let error = ManagedArchiveSession::open(&path).err().unwrap();
        assert!(!matches!(
            error,
            ArchiveError::PasswordRequired | ArchiveError::InvalidPassword
        ));
    }
}
