//! 非破壞式微調 Session (三態模型：原圖底圖、已套用結果、微調即時預覽)

use std::path::{Path, PathBuf};
use std::sync::Arc;

use core_image::{AdjustmentParams, apply_adjustments_rgba};
use image::RgbaImage;

use crate::history::HistoryManager;

/// 影像編輯 Session
#[derive(Debug)]
pub struct EditingSession {
    id: u64,
    geometry_preview: bool,
    /// 原始載入之影像底圖（不可變快照）
    original_image: Arc<RgbaImage>,
    /// 來源檔案路徑（若直接自檔案載入）
    source_path: Option<PathBuf>,
    /// 歷程管理器（維護已套用 Committed 結果與 Undo/Redo）
    history: HistoryManager,
    /// 目前滑桿調整參數
    adjustment_params: AdjustmentParams,
    /// 微調即時預覽快照（未提交至歷史前不覆寫 Committed 影像）
    preview_image: Option<Arc<RgbaImage>>,
}

impl EditingSession {
    /// 以指定影像建立新的編輯 Session
    pub fn new(image: RgbaImage, source_path: Option<PathBuf>) -> Self {
        let shared_image = Arc::new(image);
        let history = HistoryManager::new(Arc::clone(&shared_image), "開啟影像");

        Self {
            id: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            geometry_preview: false,
            original_image: Arc::clone(&shared_image),
            source_path,
            history,
            adjustment_params: AdjustmentParams::default(),
            preview_image: None,
        }
    }

    /// 取得原始影像底圖
    pub fn original_image(&self) -> Arc<RgbaImage> {
        Arc::clone(&self.original_image)
    }

    /// 取得目前已提交的基礎影像 (Committed)
    pub fn committed_image(&self) -> Arc<RgbaImage> {
        self.history.current_image()
    }

    /// 取得目前顯示影像（若有微調預覽則返回預覽，否則返回 Committed 影像）
    pub fn active_image(&self) -> Arc<RgbaImage> {
        if let Some(preview) = &self.preview_image {
            Arc::clone(preview)
        } else {
            self.committed_image()
        }
    }

    /// 取得目前來源路徑
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    /// 更新來源路徑（例如另存新檔後）
    pub fn set_source_path(&mut self, path: impl Into<PathBuf>) {
        self.source_path = Some(path.into());
    }

    /// 取得目前微調參數
    pub fn adjustment_params(&self) -> &AdjustmentParams {
        &self.adjustment_params
    }

    /// 檢查是否有進行中的微調預覽
    pub fn has_active_preview(&self) -> bool {
        self.geometry_preview
            || (self.preview_image.is_some() && !self.adjustment_params.is_identity())
    }

    pub fn identity(&self) -> (u64, u64) {
        (self.id, self.history.current_id())
    }

    pub fn begin_geometry_draft(&mut self) {
        self.geometry_preview = true;
    }

    pub fn set_geometry_preview(&mut self, image: Arc<RgbaImage>) {
        self.preview_image = Some(image);
        self.geometry_preview = true;
        self.adjustment_params = AdjustmentParams::default();
    }

    pub fn apply_geometry_preview(&mut self, description: &str) -> bool {
        if !self.geometry_preview {
            return false;
        }
        if let Some(image) = self.preview_image.take() {
            self.history.push(image, description);
            self.geometry_preview = false;
            return true;
        }
        false
    }

    pub fn set_history_budget(&mut self, bytes: usize) {
        self.history.set_budget(bytes);
    }

    pub fn preview_bytes(&self) -> usize {
        self.preview_image
            .as_ref()
            .filter(|image| !Arc::ptr_eq(image, &self.committed_image()))
            .map_or(0, |image| image.as_raw().len())
    }

    /// 即時更新微調預覽
    ///
    /// 若參數為預設值 (identity)，自動清除預覽以節省記憶體；
    /// 否則基於目前 Committed 影像進行平行微調運算並快取預覽。
    pub fn update_preview(&mut self, params: AdjustmentParams) {
        self.geometry_preview = false;
        self.adjustment_params = params;

        if params.is_identity() {
            self.preview_image = None;
        } else {
            let base = self.committed_image();
            let adjusted = apply_adjustments_rgba(&base, &params);
            self.preview_image = Some(Arc::new(adjusted));
        }
    }

    /// 設定由背景非同步運算完成之預覽影像快照
    pub fn set_preview_image(&mut self, preview: Arc<RgbaImage>, params: AdjustmentParams) {
        self.geometry_preview = false;
        self.adjustment_params = params;
        if params.is_identity() {
            self.preview_image = None;
        } else {
            self.preview_image = Some(preview);
        }
    }

    /// 套用微調：將目前預覽結果提交至 Undo 歷史，並重設微調滑桿
    pub fn apply_adjustments(&mut self) -> bool {
        if self.geometry_preview {
            return false;
        }
        if let Some(preview) = self.preview_image.take() {
            self.history.push(
                preview,
                if self.adjustment_params.curves.is_identity() {
                    "套用微調參數"
                } else {
                    "套用調色與曲線"
                },
            );
            self.adjustment_params = AdjustmentParams::default();
            true
        } else {
            false
        }
    }

    /// 取消微調：捨棄即時預覽，重設微調滑桿至預設值，完全不更動歷史
    pub fn cancel_adjustments(&mut self) {
        self.geometry_preview = false;
        self.preview_image = None;
        self.adjustment_params = AdjustmentParams::default();
    }

    /// 提交自訂動作至歷史（如手動幾何旋轉或翻轉）
    pub fn push_action(&mut self, new_image: RgbaImage, description: impl Into<String>) {
        // 先取消未套用的微調
        self.cancel_adjustments();
        self.history.push(Arc::new(new_image), description);
    }

    /// 還原回最初開啟時的原圖，並記入歷史
    pub fn revert_to_original(&mut self) {
        self.cancel_adjustments();
        let orig = self.original_image();
        self.history.push(orig, "還原至原圖");
    }

    /// 復原 (Undo)
    pub fn undo(&mut self) -> bool {
        self.cancel_adjustments();
        self.history.undo().is_some()
    }

    /// 重做 (Redo)
    pub fn redo(&mut self) -> bool {
        self.cancel_adjustments();
        self.history.redo().is_some()
    }

    /// 是否可復原
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// 是否可重做
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 是否有未存檔的變更
    pub fn is_dirty(&self) -> bool {
        self.is_committed_dirty() || self.has_active_preview()
    }

    /// 僅判斷已套用底圖是否尚未儲存，不包含未套用預覽。
    pub fn is_committed_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    /// 標記目前狀態已存檔
    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    /// 歷程管理器引用
    pub fn history(&self) -> &HistoryManager {
        &self.history
    }
}
