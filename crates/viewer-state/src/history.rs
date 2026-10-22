//! In-Memory 影像歷程管理與零磁碟 I/O 快照 Undo/Redo

use image::RgbaImage;
use std::sync::Arc;

/// 預設 Undo 歷史上限步數
pub const DEFAULT_MAX_HISTORY_STEPS: usize = 20;

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
}

impl HistoryManager {
    /// 建立新的歷程管理器，以初始影像作為基底
    pub fn new(initial_image: Arc<RgbaImage>, initial_description: impl Into<String>) -> Self {
        let initial_node = HistoryNode {
            image: initial_image,
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
