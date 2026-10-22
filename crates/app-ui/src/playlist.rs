//! 目錄檔案列表與圖片巡迴導覽管理

use std::fs;
use std::path::{Path, PathBuf};

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
    }

    /// 設定自訂圖片清單（如從壓縮包解壓出的圖片集）
    pub fn set_custom_list(&mut self, files: Vec<PathBuf>) {
        self.files = files;
        self.current_index = if self.files.is_empty() { None } else { Some(0) };
        self.current_dir = None;
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
