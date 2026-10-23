//! 目錄檔案列表與圖片巡迴導覽管理

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::{collections::HashMap, time::SystemTime};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortMode {
    #[default]
    Natural,
    Modified,
    Captured,
    Size,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortOptions {
    pub mode: SortMode,
    pub descending: bool,
}

/// 支援的圖片副檔名清單（小寫）
pub const SUPPORTED_EXTENSIONS: &[&str] =
    &["png", "jpg", "jpeg", "bmp", "webp", "tiff", "tif", "gif"];

/// 檢查路徑是否為支援的圖片格式
pub fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_lowercase();
            SUPPORTED_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum DateFilter {
    #[default]
    All,
    Day(String),
    Undated,
}
static NEXT_CATALOG_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn next_catalog_revision() -> u64 {
    NEXT_CATALOG_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// 圖片清單與導覽管理器
#[derive(Debug, Default, Clone)]
pub struct ImagePlaylist {
    /// 當前目錄下的所有圖片路徑（依檔案名稱排序）
    files: Vec<PathBuf>,
    all_files: Vec<PathBuf>,
    captured: HashMap<PathBuf, String>,
    dates_indexed: bool,
    date_filter: DateFilter,
    catalog_revision: u64,
    /// 目前正在檢視的圖片在清單中的索引
    current_index: Option<usize>,
    /// 目前清單所屬的目錄
    current_dir: Option<PathBuf>,
    attributes: HashMap<PathBuf, (u64, Option<SystemTime>)>,
    sort: SortOptions,
}

#[allow(dead_code)]
impl ImagePlaylist {
    pub fn remove_file(&mut self, path: &Path) -> Option<PathBuf> {
        let index = self.files.iter().position(|p| p == path).unwrap_or(0);
        self.files.retain(|p| p != path);
        self.attributes.remove(path);
        self.all_files.retain(|p| p != path);
        self.captured.remove(path);
        self.catalog_revision = next_catalog_revision();
        self.current_index = if self.files.is_empty() {
            None
        } else {
            Some(index.min(self.files.len() - 1))
        };
        self.current_file().cloned()
    }
    pub fn new() -> Self {
        Self::default()
    }

    /// 根據當前開啟的檔案自動掃描同目錄下的所有圖片
    pub fn scan_from_file(&mut self, current_file: &Path) {
        let parent = match current_file.parent() {
            Some(p) => p.to_path_buf(),
            None => PathBuf::from("."),
        };

        // 掃描目錄
        let mut image_files = Vec::new();
        if let Ok(entries) = fs::read_dir(&parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && is_supported_image(&path) {
                    image_files.push(path);
                }
            }
        }

        // 依檔案名稱排序 (大小寫不敏感)
        image_files.sort_by(|a, b| {
            let name_a = a.file_name().map(|n| n.to_string_lossy().to_lowercase());
            let name_b = b.file_name().map(|n| n.to_string_lossy().to_lowercase());
            name_a.cmp(&name_b)
        });

        // 定位當前檔案之索引
        let normalized_current = current_file
            .canonicalize()
            .unwrap_or_else(|_| current_file.to_path_buf());
        let current_idx = image_files.iter().position(|p| {
            p.canonicalize().unwrap_or_else(|_| p.clone()) == normalized_current
                || p == current_file
        });

        self.files = image_files;
        self.reset_catalog();
        self.current_index = current_idx;
        self.current_dir = Some(parent);
        self.refresh_metadata();
    }

    /// 設定自訂圖片清單（如從壓縮包解壓出的圖片集）
    pub fn set_custom_list(&mut self, files: Vec<PathBuf>) {
        self.files = files;
        self.reset_catalog();
        self.current_index = if self.files.is_empty() { None } else { Some(0) };
        self.current_dir = None;
        self.attributes.clear();
        self.set_sort(self.sort);
    }

    /// 只在背景載入工作使用，避免排序時於介面執行緒查詢檔案資料。
    pub fn refresh_metadata(&mut self) {
        self.catalog_revision = next_catalog_revision();
        self.captured.clear();
        self.dates_indexed = false;
        self.attributes = self
            .all_files
            .iter()
            .filter_map(|path| {
                let metadata = fs::metadata(path).ok()?;
                Some((path.clone(), (metadata.len(), metadata.modified().ok())))
            })
            .collect();
        self.set_sort(self.sort);
    }

    pub fn scan_directory(&mut self, dir: &Path) -> std::io::Result<()> {
        self.files = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && is_supported_image(path))
            .collect();
        self.reset_catalog();
        self.current_index = None;
        self.current_dir = Some(dir.to_path_buf());
        self.refresh_metadata();
        self.current_index = if self.files.is_empty() { None } else { Some(0) };
        Ok(())
    }

    pub fn set_sort(&mut self, sort: SortOptions) {
        let selected = self.current_file().cloned();
        let attributes = &self.attributes;
        let captured = &self.captured;
        self.all_files.sort_by(|a, b| {
            let natural = || {
                core_archive::natural_cmp(
                    &a.file_name().unwrap_or_default().to_string_lossy(),
                    &b.file_name().unwrap_or_default().to_string_lossy(),
                )
            };
            let primary = match sort.mode {
                SortMode::Natural => {
                    if sort.descending {
                        natural().reverse()
                    } else {
                        natural()
                    }
                }
                SortMode::Captured => {
                    compare_optional(captured.get(a), captured.get(b), sort.descending)
                }
                SortMode::Modified => compare_optional(
                    attributes.get(a).and_then(|entry| entry.1),
                    attributes.get(b).and_then(|entry| entry.1),
                    sort.descending,
                ),
                SortMode::Size => compare_optional(
                    attributes.get(a).map(|entry| entry.0),
                    attributes.get(b).map(|entry| entry.0),
                    sort.descending,
                ),
            };
            primary.then_with(natural).then_with(|| a.cmp(b))
        });
        self.sort = sort;
        self.rebuild_visible(selected.as_deref());
    }

    fn reset_catalog(&mut self) {
        self.all_files = self.files.clone();
        self.captured.clear();
        self.dates_indexed = false;
        self.date_filter = DateFilter::All;
        self.catalog_revision = next_catalog_revision();
    }
    fn rebuild_visible(&mut self, selected: Option<&Path>) {
        self.files = self
            .all_files
            .iter()
            .filter(|path| match &self.date_filter {
                DateFilter::All => true,
                DateFilter::Day(day) => self
                    .captured
                    .get(*path)
                    .is_some_and(|date| date.starts_with(day)),
                DateFilter::Undated => self.dates_indexed && !self.captured.contains_key(*path),
            })
            .cloned()
            .collect();
        self.current_index = selected.and_then(|p| self.files.iter().position(|file| file == p));
    }
    pub fn set_captured_dates(&mut self, dates: HashMap<PathBuf, String>) {
        self.captured = dates;
        self.dates_indexed = true;
        self.set_sort(self.sort);
    }
    pub fn date_filter(&self) -> &DateFilter {
        &self.date_filter
    }
    pub fn dates_indexed(&self) -> bool {
        self.dates_indexed
    }
    pub fn contains_source(&self, path: &Path) -> bool {
        self.all_files.iter().any(|file| file == path)
    }
    /// 明確開啟已知但被篩除的圖片時，清除條件以恢復導覽位置。
    pub fn select_opened_file(&mut self, path: &Path) -> bool {
        if self.try_select_existing(path) {
            return true;
        }
        if !self.contains_source(path) {
            return false;
        }
        self.set_date_filter(DateFilter::All, Some(path));
        true
    }
    pub fn set_date_filter(&mut self, filter: DateFilter, selected: Option<&Path>) {
        self.date_filter = filter;
        self.rebuild_visible(selected);
    }
    pub fn captured_at(&self, path: &Path) -> Option<&str> {
        self.captured.get(path).map(String::as_str)
    }
    pub fn all_files(&self) -> &[PathBuf] {
        &self.all_files
    }
    pub fn catalog_revision(&self) -> u64 {
        self.catalog_revision
    }

    /// 在現有清單中更新目前檔案之索引（若檔案已在清單中則不重新掃描目錄）
    pub fn try_select_existing(&mut self, current_file: &Path) -> bool {
        if let Some(idx) = self.files.iter().position(|p| p == current_file) {
            self.current_index = Some(idx);
            true
        } else {
            self.current_index = None;
            false
        }
    }

    /// 取得檔案總數
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// 清單是否為空
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// 取得目前索引 (0-based)
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    /// 取得所有檔案路徑列表切片
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// 快取範圍識別不讀取磁碟；壓縮清單以受管理的暫存目錄區分。
    pub fn cache_scope(&self) -> Option<PathBuf> {
        self.current_dir
            .clone()
            .or_else(|| self.all_files.first()?.parent().map(Path::to_path_buf))
    }

    /// 取得目前指向的檔案路徑
    pub fn current_file(&self) -> Option<&PathBuf> {
        self.current_index.and_then(|idx| self.files.get(idx))
    }

    /// 切換至上一張圖片
    pub fn prev(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() {
            return None;
        }

        let new_idx = match self.current_index {
            Some(idx) if idx > 0 => idx - 1,
            Some(_) => self.files.len() - 1, // 循環至末端
            None => 0,
        };

        self.current_index = Some(new_idx);
        self.files.get(new_idx).cloned()
    }

    /// 切換至下一張圖片
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() {
            return None;
        }

        let new_idx = match self.current_index {
            Some(idx) if idx + 1 < self.files.len() => idx + 1,
            Some(_) => 0, // 循環回前端
            None => 0,
        };

        self.current_index = Some(new_idx);
        self.files.get(new_idx).cloned()
    }

    /// 跳轉至指定索引
    pub fn jump_to(&mut self, index: usize) -> Option<PathBuf> {
        if index < self.files.len() {
            self.current_index = Some(index);
            self.files.get(index).cloned()
        } else {
            None
        }
    }

    /// 跳轉至第一張
    pub fn first(&mut self) -> Option<PathBuf> {
        self.jump_to(0)
    }

    /// 跳轉至最後一張
    pub fn last(&mut self) -> Option<PathBuf> {
        if self.files.is_empty() {
            None
        } else {
            self.jump_to(self.files.len() - 1)
        }
    }
}

fn compare_optional<T: Ord>(a: Option<T>, b: Option<T>, descending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (Some(a), Some(b)) => {
            if descending {
                b.cmp(&a)
            } else {
                a.cmp(&b)
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn 未完成日期索引不得把尚未讀取的來源當成無日期() {
        let mut list = ImagePlaylist::new();
        let a = PathBuf::from("有日期.png");
        let b = PathBuf::from("無日期.png");
        list.set_custom_list(vec![a.clone(), b.clone()]);
        list.set_date_filter(DateFilter::Undated, None);
        assert!(list.is_empty());
        let dates = HashMap::from([(a, "2025-01-01 10:00:00".into())]);
        list.set_captured_dates(dates.clone());
        assert_eq!(list.files(), std::slice::from_ref(&b));
        list.refresh_metadata();
        assert!(list.is_empty());
        assert_eq!(list.date_filter(), &DateFilter::Undated);
        list.set_captured_dates(dates);
        assert_eq!(list.files(), std::slice::from_ref(&b));
    }
    #[test]
    fn natural_and_metadata_sorts_preserve_selection() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("相片2.png");
        let b = dir.path().join("相片10.png");
        let c = dir.path().join("相片100000000000000000000.png");
        fs::write(&a, [0; 3]).unwrap();
        fs::write(&b, [0; 1]).unwrap();
        fs::write(&c, [0; 2]).unwrap();
        let mut list = ImagePlaylist::new();
        list.scan_directory(dir.path()).unwrap();
        assert_eq!(list.files(), &[a.clone(), b.clone(), c.clone()]);
        list.try_select_existing(&b);
        list.set_sort(SortOptions {
            mode: SortMode::Size,
            descending: false,
        });
        assert_eq!(list.files(), &[b.clone(), c.clone(), a.clone()]);
        assert_eq!(list.current_file(), Some(&b));
        list.set_sort(SortOptions {
            mode: SortMode::Natural,
            descending: true,
        });
        assert_eq!(list.files(), &[c.clone(), b.clone(), a.clone()]);
        assert_eq!(list.current_file(), Some(&b));
        for (path, seconds) in [(&a, 100), (&b, 300), (&c, 200)] {
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
                .unwrap();
        }
        list.refresh_metadata();
        list.set_sort(SortOptions {
            mode: SortMode::Modified,
            descending: false,
        });
        assert_eq!(list.files(), &[a, c, b.clone()]);
        assert_eq!(list.current_file(), Some(&b));
        assert_eq!(
            core_archive::natural_cmp("x100000000000000000000", "x99999999999999999999"),
            std::cmp::Ordering::Greater
        );
    }
    #[test]
    fn 拍攝排序未知置後且篩選可恢復完整清單與選取() {
        let mut list = ImagePlaylist::new();
        let paths: Vec<PathBuf> = ["相片10.jpg", "相片2.jpg", "早期.jpg", "未知.jpg"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        list.set_custom_list(paths.clone());
        let revision = list.catalog_revision();
        list.set_captured_dates(HashMap::from([
            (paths[0].clone(), "2025-01-02 10:00:00".into()),
            (paths[1].clone(), "2025-01-02 10:00:00".into()),
            (paths[2].clone(), "2024-12-01 10:00:00".into()),
        ]));
        list.try_select_existing(&paths[0]);
        list.set_sort(SortOptions {
            mode: SortMode::Captured,
            descending: false,
        });
        assert_eq!(
            list.files(),
            &[
                paths[2].clone(),
                paths[1].clone(),
                paths[0].clone(),
                paths[3].clone()
            ]
        );
        list.set_sort(SortOptions {
            mode: SortMode::Captured,
            descending: true,
        });
        assert_eq!(
            list.files(),
            &[
                paths[1].clone(),
                paths[0].clone(),
                paths[2].clone(),
                paths[3].clone()
            ]
        );
        assert_eq!(list.current_file(), Some(&paths[0]));
        list.set_date_filter(DateFilter::Day("2024-12-01".into()), Some(&paths[0]));
        assert_eq!(list.files(), &[paths[2].clone()]);
        assert!(list.current_index().is_none());
        list.set_date_filter(DateFilter::Undated, Some(&paths[0]));
        assert_eq!(list.files(), &[paths[3].clone()]);
        list.set_date_filter(DateFilter::All, Some(&paths[0]));
        assert_eq!(list.current_file(), Some(&paths[0]));
        assert_eq!(list.catalog_revision(), revision);
        assert_eq!(list.all_files().len(), 4);
        list.set_date_filter(DateFilter::Day("1990-01-01".into()), Some(&paths[0]));
        assert!(list.next().is_none());
        assert!(list.prev().is_none());
        list.set_date_filter(DateFilter::All, Some(&paths[0]));
        list.remove_file(&paths[1]);
        assert_eq!(list.all_files().len(), 3);
        assert_ne!(list.catalog_revision(), revision);
    }
}
