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

/// 圖片清單與導覽管理器
#[derive(Debug, Default, Clone)]
pub struct ImagePlaylist {
    /// 當前目錄下的所有圖片路徑（依檔案名稱排序）
    files: Vec<PathBuf>,
    /// 目前正在檢視的圖片在清單中的索引
    current_index: Option<usize>,
    /// 目前清單所屬的目錄
    current_dir: Option<PathBuf>,
    attributes: HashMap<PathBuf, (u64, Option<SystemTime>)>,
    sort: SortOptions,
}

#[allow(dead_code)]
impl ImagePlaylist {
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
        self.current_index = current_idx;
        self.current_dir = Some(parent);
        self.refresh_metadata();
    }

    /// 設定自訂圖片清單（如從壓縮包解壓出的圖片集）
    pub fn set_custom_list(&mut self, files: Vec<PathBuf>) {
        self.files = files;
        self.current_index = if self.files.is_empty() { None } else { Some(0) };
        self.current_dir = None;
        self.attributes.clear();
        self.set_sort(self.sort);
    }

    /// 只在背景載入工作使用，避免排序時於介面執行緒查詢檔案資料。
    pub fn refresh_metadata(&mut self) {
        self.attributes = self
            .files
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
        self.current_index = None;
        self.current_dir = Some(dir.to_path_buf());
        self.refresh_metadata();
        self.current_index = if self.files.is_empty() { None } else { Some(0) };
        Ok(())
    }

    pub fn set_sort(&mut self, sort: SortOptions) {
        let selected = self.current_file().cloned();
        let attributes = &self.attributes;
        self.files.sort_by(|a, b| {
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
        self.current_index =
            selected.and_then(|path| self.files.iter().position(|candidate| candidate == &path));
        self.sort = sort;
    }

    /// 在現有清單中更新目前檔案之索引（若檔案已在清單中則不重新掃描目錄）
    pub fn try_select_existing(&mut self, current_file: &Path) -> bool {
        if let Some(idx) = self.files.iter().position(|p| p == current_file) {
            self.current_index = Some(idx);
            true
        } else {
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
            .or_else(|| self.files.first()?.parent().map(Path::to_path_buf))
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
}
