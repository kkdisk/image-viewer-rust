//! In-Memory 影像歷程管理與零磁碟 I/O 快照 Undo/Redo

use image::RgbaImage;
use std::sync::Arc;

/// 預設 Undo 歷史上限步數
pub const DEFAULT_MAX_HISTORY_STEPS: usize = 20;
/// 歷史保留的軟預算，必要原圖及目前底圖可超出此值。
pub const DEFAULT_HISTORY_BYTES: usize = 512 * 1024 * 1024;

/// 歷程節點
#[derive(Debug, Clone)]
pub struct HistoryNode {
    /// 該步驟之記憶體影像快照
    pub image: Arc<RgbaImage>,
    /// 步驟動作描述（如 "套用色彩微調", "旋轉 90 度"）
    pub description: String,
    /// 內部節點流水號，用於精準判斷未存檔 (dirty) 狀態
    pub id: u64,
}

/// 記憶體內部 Undo/Redo 歷程管理器
#[derive(Debug)]
pub struct HistoryManager {
    max_steps: usize,
    past: Vec<HistoryNode>,
    current: HistoryNode,
    future: Vec<HistoryNode>,
    next_id: u64,
    saved_id: u64,
    original: Arc<RgbaImage>,
    budget: usize,
}

impl HistoryManager {
    /// 建立新的歷程管理器，以初始影像作為基底
    pub fn new(initial_image: Arc<RgbaImage>, initial_description: impl Into<String>) -> Self {
        let initial_node = HistoryNode {
            image: Arc::clone(&initial_image),
            description: initial_description.into(),
            id: 0,
        };

        Self {
            max_steps: DEFAULT_MAX_HISTORY_STEPS,
            past: Vec::new(),
            current: initial_node,
            future: Vec::new(),
            next_id: 1,
            saved_id: 0,
            original: initial_image,
            budget: DEFAULT_HISTORY_BYTES,
        }
    }

    /// 設定最大歷程保留步數
    pub fn with_max_steps(mut self, steps: usize) -> Self {
        self.max_steps = steps.max(1);
        while self.past.len() > self.max_steps {
            self.past.remove(0);
        }
        self
    }

    /// 取得目前步數上限
    pub fn max_steps(&self) -> usize {
        self.max_steps
    }

    pub fn current_id(&self) -> u64 {
        self.current.id
    }

    /// 保留影像的配置去重計量，不包含預覽、工作中間值與 GPU。
    pub fn retained_bytes(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        std::iter::once(&self.original)
            .chain(std::iter::once(&self.current.image))
            .chain(self.past.iter().map(|n| &n.image))
            .chain(self.future.iter().map(|n| &n.image))
            .filter(|image| seen.insert(Arc::as_ptr(image)))
            .map(|image| image.as_raw().len())
            .sum()
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    pub fn set_budget(&mut self, bytes: usize) {
        self.budget = bytes;
        self.trim_budget();
    }

    fn trim_budget(&mut self) {
        while self.retained_bytes() > self.budget && !self.past.is_empty() {
            self.past.remove(0);
        }
        // future 的尾端是最近重做，索引零是最遠的未來節點。
        while self.retained_bytes() > self.budget && !self.future.is_empty() {
            self.future.remove(0);
        }
    }

    /// 取得目前影像快照引用
    pub fn current_image(&self) -> Arc<RgbaImage> {
        Arc::clone(&self.current.image)
    }

    /// 取得目前節點動作描述
    pub fn current_description(&self) -> &str {
        &self.current.description
    }

    /// 檢查是否可復原 (Undo)
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    /// 檢查是否可重做 (Redo)
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    /// 可復原步數
    pub fn undo_len(&self) -> usize {
        self.past.len()
    }

    /// 可重做步數
    pub fn redo_len(&self) -> usize {
        self.future.len()
    }

    /// 提交新動作：推入歷史並清除 Redo 分支
    pub fn push(&mut self, image: Arc<RgbaImage>, description: impl Into<String>) {
        let new_id = self.next_id;
        self.next_id += 1;

        let new_node = HistoryNode {
            image,
            description: description.into(),
            id: new_id,
        };

        let old_node = std::mem::replace(&mut self.current, new_node);
        self.past.push(old_node);

        // 超過上限時移除最舊記錄
        if self.past.len() > self.max_steps {
            self.past.remove(0);
        }

        // 提交新動作時分支清除
        self.future.clear();
        self.trim_budget();
    }

    /// 執行復原 (Undo)
    pub fn undo(&mut self) -> Option<Arc<RgbaImage>> {
        if let Some(prev_node) = self.past.pop() {
            let old_current = std::mem::replace(&mut self.current, prev_node);
            self.future.push(old_current);
            Some(Arc::clone(&self.current.image))
        } else {
            None
        }
    }

    /// 執行重做 (Redo)
    pub fn redo(&mut self) -> Option<Arc<RgbaImage>> {
        if let Some(next_node) = self.future.pop() {
            let old_current = std::mem::replace(&mut self.current, next_node);
            self.past.push(old_current);
            Some(Arc::clone(&self.current.image))
        } else {
            None
        }
    }

    /// 標記目前狀態已存檔
    pub fn mark_saved(&mut self) {
        self.saved_id = self.current.id;
    }

    /// 檢查自上次存檔後是否有任何未儲存之變更
    pub fn is_dirty(&self) -> bool {
        self.current.id != self.saved_id
    }
}
