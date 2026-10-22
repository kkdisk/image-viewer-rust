//! eframe::App 主桌面應用邏輯

use core_image::AdjustmentParams;
use egui::{CentralPanel, Key, TextureHandle};
use std::path::PathBuf;
use std::sync::Arc;
use viewer_state::{EditingSession, ViewportState};

use crate::canvas::{
    CanvasState, CanvasView, ComparisonAction, ComparisonConfig, ComparisonSource,
};
use crate::debounce_queue::AdjustmentDebouncer;
use crate::panels::{
    AboutDialog, AdjustmentPanel, AdjustmentPanelState, DuplicateFinderDialog,
    DuplicateFinderState, FilmstripPanel, MenuBar, StatusBar,
};
use crate::playlist::ImagePlaylist;
use crate::theme::apply_theme;
use crate::thumbnail_loader::ThumbnailManager;

/// 主圖片瀏覽與編輯桌面應用狀態
pub struct ImageViewerApp {
    pub(crate) animated_source: bool,
    pub(crate) animation_save: Option<crate::animation_save::AnimationSave>,
    pub(crate) leave_guard: crate::navigation_guard::LeaveGuard,
    pub(crate) document_io: crate::document_io::DocumentIo,
    pub(crate) preferences: crate::preferences::PreferenceState,
    pub white_balance_open: bool,
    pub fine_tune_open: bool,
    /// 圖片編輯 Session
    pub session: Option<EditingSession>,
    /// 視口縮放與平移狀態
    pub viewport: ViewportState,
    /// 側邊調整欄開關
    pub show_side_panel: bool,
    /// 底部縮圖列開關
    pub show_filmstrip: bool,
    /// 關於對話框顯示開關
    pub show_about_dialog: bool,
    /// 重複圖片檢查器狀態
    pub duplicate_finder_state: DuplicateFinderState,
    /// GPU 影像紋理快取
    pub texture: Option<TextureHandle>,
    /// 主圖背景顯示預覽，持有來源快照並核對裝置上限。
    active_display: crate::display_texture::DisplayTexture,
    /// 目錄圖片清單與巡迴導覽管理器
    pub playlist: ImagePlaylist,
    /// 非同步縮圖生成與快取管理器
    pub thumbnail_manager: ThumbnailManager,
    /// 側邊微調面板目前滑桿參數
    pub slider_params: AdjustmentParams,
    /// 像素微調防抖背景運算排程器
    pub debouncer: AdjustmentDebouncer,
    pending_adjustment: Option<u64>,
    /// 雙圖比對視圖配置
    pub comparison: ComparisonConfig,
    /// 雙圖比對時左側獨立視口（當未開啟同步時使用）
    pub left_viewport: ViewportState,
    /// 原始底圖 GPU 紋理（用於原圖比對）
    pub original_texture: Option<TextureHandle>,
    /// 原圖與參考圖各自隔離的背景顯示工作。
    original_display: crate::display_texture::DisplayTexture,
    reference_display: crate::display_texture::DisplayTexture,
    /// 外部參考圖 RGBA 快照
    pub reference_image: Option<Arc<image::RgbaImage>>,
    /// 外部參考圖 GPU 紋理
    pub reference_texture: Option<TextureHandle>,
    /// 受管理的壓縮檔瀏覽 Session
    pub archive_session: Option<core_archive::ManagedArchiveSession>,
    /// 待解密之壓縮檔路徑
    pub pending_archive_path: Option<PathBuf>,
    /// 壓縮檔解密密碼輸入緩衝
    pub archive_password_input: String,
    /// 密碼錯誤或解壓錯誤提示
    pub archive_password_error: Option<String>,
    /// 是否以明文顯示密碼
    pub archive_show_password: bool,
    /// 是否顯示密碼輸入彈出視窗
    pub show_password_dialog: bool,
    /// 壓縮檔非同步背景解壓縮排程器
    pub archive_loader: crate::archive_loader::ArchiveLoader,
    /// 最新發起之壓縮檔解壓請求 ID
    pub latest_archive_req_id: u64,
    /// 最後已由介面接收的背景請求，避免等待函式早於結果交付返回
    received_archive_req_id: u64,
    /// 全域通知訊息與產生時間戳記 (訊息, 產生時間)
    pub status_notification: Option<(String, std::time::Instant)>,
    /// 直方圖快取數據
    pub cached_histogram: Option<core_image::HistogramData>,
    /// 最近送往直方圖工作的影像，持有快照避免位址重用誤判
    last_histogram_image: Option<Arc<image::RgbaImage>>,
    histogram_loader: crate::histogram_loader::HistogramLoader,
    histogram_pending: bool,
    /// 直方圖顯示模式 (RGB 疊加、純 R、純 G、純 B、Luma)
    pub histogram_mode: crate::panels::HistogramMode,
    /// 是否啟用局部放大鏡 (快捷鍵 M)
    pub magnifier_enabled: bool,
    /// 局部放大鏡縮放倍率 (預設 4x，可切換 2x, 4x, 8x)
    pub magnifier_zoom: u32,
}

impl Default for ImageViewerApp {
    fn default() -> Self {
        Self {
            animated_source: false,
            animation_save: None,
            leave_guard: crate::navigation_guard::LeaveGuard::default(),
            document_io: crate::document_io::DocumentIo::default(),
            preferences: crate::preferences::PreferenceState::default(),
            white_balance_open: true,
            fine_tune_open: true,
            session: None,
            viewport: ViewportState::new(),
            show_side_panel: true,
            show_filmstrip: true,
            show_about_dialog: false,
            duplicate_finder_state: DuplicateFinderState::default(),
            texture: None,
            active_display: Default::default(),
            playlist: ImagePlaylist::new(),
            thumbnail_manager: ThumbnailManager::new(),
            slider_params: AdjustmentParams::default(),
            debouncer: AdjustmentDebouncer::new(),
            pending_adjustment: None,
            comparison: ComparisonConfig::default(),
            left_viewport: ViewportState::new(),
            original_texture: None,
            original_display: Default::default(),
            reference_display: Default::default(),
            reference_image: None,
            reference_texture: None,
            archive_session: None,
            pending_archive_path: None,
            archive_password_input: String::new(),
            archive_password_error: None,
            archive_show_password: false,
            show_password_dialog: false,
            archive_loader: crate::archive_loader::ArchiveLoader::new(),
            latest_archive_req_id: 0,
            received_archive_req_id: 0,
            status_notification: None,
            cached_histogram: None,
            last_histogram_image: None,
            histogram_loader: crate::histogram_loader::HistogramLoader::default(),
            histogram_pending: false,
            histogram_mode: crate::panels::HistogramMode::default(),
            magnifier_enabled: false,
            magnifier_zoom: 4,
        }
    }
}

impl ImageViewerApp {
    pub(crate) fn is_preview_pending(&self) -> bool {
        self.pending_adjustment.is_some()
    }
    /// 建立應用實例
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 套用自訂現代暗色主題與微軟正黑體字型
        apply_theme(&cc.egui_ctx);
        Self::default()
    }

    /// 開啟指定路徑之影像或壓縮檔（.zip / .7z）
    pub fn open_file_path(&mut self, path: PathBuf) -> Result<(), String> {
        if self.is_saving() {
            return Err("儲存進行中，請稍候再開啟檔案".into());
        }
        if self.has_pending_leave() {
            return Ok(());
        }
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if ext == "zip" || ext == "7z" {
            self.cancel_archive_loading();
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("壓縮檔")
                .to_string();
            self.status_notification = Some((
                format!("正在背景解壓縮 '{}'...", filename),
                std::time::Instant::now(),
            ));
            // 分發至背景解壓工作池，UI 主執行緒立即返回，絕不凍結
            self.latest_archive_req_id = self.archive_loader.load_archive(path, None);
            Ok(())
        } else {
            self.open_image_path(path)
                .map_err(|e| format!("無法載入圖片: {}", e))
        }
    }

    /// 使用指定密碼在背景解壓並開啟壓縮檔
    pub fn open_archive_with_password(
        &mut self,
        path: PathBuf,
        password: &str,
    ) -> Result<(), String> {
        self.cancel_archive_loading();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("壓縮檔")
            .to_string();
        self.status_notification = Some((
            format!("正在背景解密 '{}'...", filename),
            std::time::Instant::now(),
        ));
        // 分發至背景執行緒解密
        self.latest_archive_req_id = self
            .archive_loader
            .load_archive(path, Some(password.to_string()));
        Ok(())
    }

    /// 套用已解壓完成之壓縮檔 Session 至播放清單與首張圖片檢視
    pub fn apply_archive_session(
        &mut self,
        session: core_archive::ManagedArchiveSession,
    ) -> Result<(), String> {
        if self.has_pending_leave() {
            return Ok(());
        }
        if self.has_unsaved_changes() {
            self.defer_leave(crate::navigation_guard::LeaveAction::Archive(session));
            return Ok(());
        }
        self.apply_archive_session_unchecked(session)
    }

    pub(crate) fn apply_archive_session_unchecked(
        &mut self,
        session: core_archive::ManagedArchiveSession,
    ) -> Result<(), String> {
        let first_img = session
            .image_paths
            .first()
            .cloned()
            .ok_or_else(|| "壓縮檔內未找到支援的圖片".to_string())?;

        self.cancel_archive_loading();
        self.enqueue_image(first_img, Some(session), false)
    }

    /// 輪詢背景壓縮檔解壓結果並更新應用狀態（回傳是否有狀態變更）
    pub fn poll_archive_loader(&mut self) -> bool {
        if let Some(archive_res) = self.archive_loader.poll() {
            self.received_archive_req_id = archive_res.request_id;
            if archive_res.request_id != self.latest_archive_req_id {
                return true;
            }
            match archive_res.result {
                Ok(session) => {
                    let count = session.image_paths.len();
                    if let Err(e) = self.apply_archive_session(session) {
                        self.status_notification = Some((
                            format!("載入解壓圖片失敗: {}", e),
                            std::time::Instant::now(),
                        ));
                    } else {
                        self.pending_archive_path = None;
                        self.show_password_dialog = false;
                        self.archive_password_error = None;
                        self.archive_password_input.clear();
                        self.status_notification = Some((
                            if self.has_pending_leave() {
                                "解壓完成，等待處理目前未儲存的變更".to_owned()
                            } else {
                                format!("已解壓載入壓縮檔 (共 {} 張圖片)", count)
                            },
                            std::time::Instant::now(),
                        ));
                    }
                }
                Err(core_archive::ArchiveError::PasswordRequired) => {
                    self.pending_archive_path = Some(archive_res.path);
                    self.show_password_dialog = true;
                    self.archive_password_error = None;
                    self.status_notification = Some((
                        "此壓縮檔受密碼保護，請輸入解鎖密碼".to_string(),
                        std::time::Instant::now(),
                    ));
                }
                Err(core_archive::ArchiveError::InvalidPassword) => {
                    self.pending_archive_path = Some(archive_res.path);
                    self.show_password_dialog = true;
                    self.archive_password_error =
                        Some("密碼錯誤或解密失敗，請檢查密碼".to_string());
                    self.status_notification = Some((
                        "密碼錯誤，請重新輸入解鎖密碼".to_string(),
                        std::time::Instant::now(),
                    ));
                }
                Err(e) => {
                    let msg = format!("無法開啟壓縮檔: {}", e);
                    self.archive_password_error = Some(msg.clone());
                    self.status_notification = Some((msg, std::time::Instant::now()));
                }
            }
            return true;
        }
        false
    }

    /// 等候當前背景解壓完成（供測試或精確同步使用，最多等待 timeout）
    pub fn wait_archive_loaded(&mut self, timeout: std::time::Duration) -> bool {
        let target_id = self.latest_archive_req_id;
        let start = std::time::Instant::now();
        while start.elapsed() < timeout {
            self.poll_archive_loader();
            self.poll_document_io();
            if self.received_archive_req_id >= target_id && !self.io_needs_repaint() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    /// 開啟指定路徑之影像，並視情況維護播放清單
    pub fn open_image_path(&mut self, path: PathBuf) -> Result<(), core_image::ImageError> {
        if self.defer_leave(crate::navigation_guard::LeaveAction::Image(path.clone())) {
            return Ok(());
        }
        self.open_image_path_unchecked(path)
    }

    pub(crate) fn open_image_path_unchecked(
        &mut self,
        path: PathBuf,
    ) -> Result<(), core_image::ImageError> {
        self.cancel_archive_loading();
        self.enqueue_image(path, None, false)
            .map_err(|error| core_image::ImageError::Io(std::io::Error::other(error)))
    }

    pub(crate) fn install_loaded_image(&mut self, loaded: crate::document_io::LoadedImage) {
        if loaded.reference {
            self.reference_image = Some(Arc::new(loaded.pixels));
            self.reference_texture = None;
            self.comparison.reference_path = Some(loaded.path);
            self.comparison.source = ComparisonSource::Reference;
            self.comparison.is_active = true;
            return;
        }
        self.animated_source = loaded.animated;
        if let Some(archive) = loaded.archive {
            self.archive_session = Some(archive);
        } else if !self
            .archive_session
            .as_ref()
            .is_some_and(|archive| archive.is_from_archive(&loaded.path))
        {
            self.archive_session = None;
        }
        self.debouncer.cancel_pending();
        self.pending_adjustment = None;
        self.thumbnail_manager.clear();
        self.playlist = loaded.playlist;
        self.session = Some(EditingSession::new(loaded.pixels, Some(loaded.path)));
        self.slider_params = AdjustmentParams::default();
        self.viewport.reset_fit_window();
        self.left_viewport = self.viewport;
        self.original_texture = None;
        self.original_display.clear();
        self.reference_texture = None;
        self.reference_image = None;
        self.comparison.reference_path = None;
        self.document_io.cancel_reference();
        self.status_notification = Some(("圖片已載入".into(), std::time::Instant::now()));
    }

    /// 新開檔、導覽或取消時讓所有舊背景結果失效
    pub fn cancel_archive_loading(&mut self) {
        self.document_io.cancel_main();
        self.latest_archive_req_id = 0;
        self.pending_archive_path = None;
        self.show_password_dialog = false;
        self.archive_password_input.clear();
        self.archive_password_error = None;
    }

    /// 彈出原生系統檔案選取對話框
    pub fn open_file_dialog(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter(
                "支援的檔案 (*.png, *.jpg, *.jpeg, *.bmp, *.webp, *.zip, *.7z)",
                &[
                    "png", "jpg", "jpeg", "bmp", "webp", "tiff", "gif", "zip", "7z",
                ],
            )
            .add_filter(
                "圖片檔案 (*.png, *.jpg, *.jpeg, *.bmp, *.webp, *.tiff, *.gif)",
                &["png", "jpg", "jpeg", "bmp", "webp", "tiff", "gif"],
            )
            .add_filter("壓縮檔案 (*.zip, *.7z)", &["zip", "7z"])
            .add_filter("所有檔案 (*.*)", &["*"])
            .pick_file();

        if let Some(path) = file
            && let Err(e) = self.open_file_path(path)
        {
            self.status_notification = Some((e, std::time::Instant::now()));
        }
    }

    /// 開啟外部參考圖檔案對話框
    pub fn open_reference_image_dialog(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter(
                "圖片檔案 (*.png, *.jpg, *.jpeg, *.bmp, *.webp, *.tiff, *.gif)",
                &["png", "jpg", "jpeg", "bmp", "webp", "tiff", "gif"],
            )
            .add_filter("所有檔案 (*.*)", &["*"])
            .pick_file();

        if let Some(path) = file {
            let _ = self.open_reference_image_path(path);
        }
    }

    pub fn open_reference_image_path(&mut self, path: PathBuf) -> Result<(), String> {
        self.enqueue_image(path, None, true)
    }

    /// 切換雙圖分割比對模式
    pub fn toggle_side_by_side(&mut self) {
        self.comparison.is_active = !self.comparison.is_active;
        if self.comparison.is_active && self.comparison.sync_viewport {
            self.left_viewport = self.viewport;
        }
    }

    /// 切換至上一張圖片
    pub fn navigate_prev(&mut self) {
        if !self.playlist.is_empty() {
            let index = self
                .document_io
                .intended_index(&self.playlist)
                .map(|i| (i + self.playlist.len() - 1) % self.playlist.len())
                .unwrap_or(0);
            self.navigate_to(index);
        }
    }

    /// 切換至下一張圖片
    pub fn navigate_next(&mut self) {
        if !self.playlist.is_empty() {
            let index = self
                .document_io
                .intended_index(&self.playlist)
                .map(|i| (i + 1) % self.playlist.len())
                .unwrap_or(0);
            self.navigate_to(index);
        }
    }

    /// 切換至指定索引之圖片
    pub fn navigate_to(&mut self, index: usize) {
        if let Some(path) = self.playlist.files().get(index).cloned()
            && let Err(error) = self.open_image_path(path)
        {
            self.status_notification = Some((error.to_string(), std::time::Instant::now()));
        }
    }

    /// 更新 GPU 影像紋理（主視口與雙圖左側視口）
    fn sync_texture(&mut self, ctx: &egui::Context) {
        if let Some(session) = &self.session {
            self.active_display.sync(
                ctx,
                session.active_image(),
                &mut self.texture,
                "active_viewport_image",
            );
            if self.comparison.is_active {
                match self.comparison.source {
                    ComparisonSource::Original => self.original_display.sync(
                        ctx,
                        session.original_image(),
                        &mut self.original_texture,
                        "original_viewport_image",
                    ),
                    ComparisonSource::Reference => {
                        if let Some(image) = &self.reference_image {
                            self.reference_display.sync(
                                ctx,
                                Arc::clone(image),
                                &mut self.reference_texture,
                                "reference_viewport_image",
                            );
                        } else {
                            self.reference_texture = None;
                            self.reference_display.clear();
                        }
                    }
                }
            }
        } else {
            self.texture = None;
            self.original_texture = None;
            self.active_display.clear();
            self.original_display.clear();
        }
    }

    /// 提交背景直方圖運算並接收符合目前影像的結果，不等待像素統計
    pub fn sync_histogram(&mut self) {
        if !self.show_side_panel {
            return;
        }
        if let Some(sess) = &self.session {
            let active = sess.active_image();
            if !self
                .last_histogram_image
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, &active))
            {
                self.histogram_loader.request(Arc::clone(&active));
                self.last_histogram_image = Some(Arc::clone(&active));
                self.histogram_pending = true;
            }
            while let Some(result) = self.histogram_loader.poll() {
                if Arc::ptr_eq(&result.image, &active) {
                    self.cached_histogram = Some(result.histogram);
                    self.histogram_pending = false;
                }
            }
        } else {
            self.cached_histogram = None;
            self.last_histogram_image = None;
            self.histogram_pending = false;
            while self.histogram_loader.poll().is_some() {}
        }
    }

    /// 僅在背景預覽完成後套用，並同步重設介面參數。
    pub fn apply_current_adjustments(&mut self) -> bool {
        if self.is_saving() {
            return false;
        }
        if self.pending_adjustment.is_some() || self.debouncer.is_computing() {
            return false;
        }
        let applied = self
            .session
            .as_mut()
            .is_some_and(|session| session.apply_adjustments());
        if applied {
            self.slider_params = AdjustmentParams::default();
            self.debouncer.cancel_pending();
        }
        applied
    }

    /// 復原上一動作
    pub fn undo(&mut self) {
        if self.is_saving() {
            return;
        }
        self.debouncer.cancel_pending();
        self.pending_adjustment = None;
        if let Some(session) = &mut self.session
            && session.undo()
        {
            self.slider_params = AdjustmentParams::default();
        }
    }

    /// 重做下一動作
    pub fn redo(&mut self) {
        if self.is_saving() {
            return;
        }
        self.debouncer.cancel_pending();
        self.pending_adjustment = None;
        if let Some(session) = &mut self.session
            && session.redo()
        {
            self.slider_params = AdjustmentParams::default();
        }
    }

    /// 檢查目前開啟的圖片是否來自壓縮包暫存目錄
    pub fn is_current_image_from_archive(&self) -> bool {
        if let Some(session) = &self.session
            && let Some(path) = session.source_path()
        {
            return self
                .archive_session
                .as_ref()
                .map(|arc| arc.is_from_archive(path))
                .unwrap_or(false);
        }
        false
    }

    /// 提交原子寫檔；回傳是否已排程，實際成功由背景交付確認。
    pub fn save_image_to(&mut self, target_path: &std::path::Path) -> bool {
        if !self.can_save_committed_image() {
            return false;
        }
        if self
            .archive_session
            .as_ref()
            .is_some_and(|archive| archive.is_from_archive(target_path))
        {
            self.status_notification = Some((
                "壓縮檔內圖片必須另存至外部位置".into(),
                std::time::Instant::now(),
            ));
            return false;
        }
        if self.is_saving() || self.is_loading_image() {
            return false;
        }
        if self.animated_source
            && let Some(session) = &self.session
        {
            self.animation_save = Some(crate::animation_save::AnimationSave {
                path: target_path.to_path_buf(),
                source: session.committed_image(),
            });
            return true;
        }
        self.enqueue_save(target_path.to_path_buf())
    }

    /// 原地原子儲存已提交之影像
    pub fn save_current_image(&mut self) -> bool {
        if !self.can_save_committed_image() {
            return false;
        }
        if self.is_current_image_from_archive() {
            // 若當前檔案來自壓縮包，嚴禁原地覆寫，強制導向另存新檔！
            return self.save_image_as_dialog();
        }

        if let Some(path) = self
            .session
            .as_ref()
            .and_then(|s| s.source_path())
            .map(|p| p.to_path_buf())
        {
            self.save_image_to(&path)
        } else {
            self.save_image_as_dialog()
        }
    }

    /// 彈出原生對話框另存新檔
    pub fn save_image_as_dialog(&mut self) -> bool {
        if !self.can_save_committed_image() {
            return false;
        }
        if let Some(session) = &self.session {
            let dialog = rfd::FileDialog::new()
                .add_filter("PNG 影像 (*.png)", &["png"])
                .add_filter("JPEG 影像 (*.jpg, *.jpeg)", &["jpg", "jpeg"])
                .add_filter("BMP 影像 (*.bmp)", &["bmp"])
                .add_filter("WebP 影像 (*.webp)", &["webp"])
                .add_filter("TIFF 影像 (*.tiff)", &["tiff"]);

            let dialog = if let Some(src) = session.source_path() {
                if let Some(parent) = src.parent() {
                    dialog.set_directory(parent)
                } else {
                    dialog
                }
            } else {
                dialog
            };

            if let Some(target_path) = dialog.save_file() {
                return self.save_image_to(&target_path);
            }
        }
        false
    }

    fn can_save_committed_image(&mut self) -> bool {
        let preview = self
            .session
            .as_ref()
            .is_some_and(|s| s.has_active_preview())
            || !self.slider_params.is_identity();
        if preview {
            self.status_notification = Some((
                "請先套用或取消預覽，再儲存圖片".into(),
                std::time::Instant::now(),
            ));
        }
        self.session.is_some() && !preview
    }

    /// 處理鍵盤快捷鍵
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.has_pending_leave() || ctx.wants_keyboard_input() {
            return;
        }
        let mut do_prev = false;
        let mut do_next = false;
        let mut do_first = false;
        let mut do_last = false;
        let mut do_undo = false;
        let mut do_redo = false;
        let mut do_save = false;
        let mut do_save_as = false;

        ctx.input(|i| {
            // Ctrl + O: 開啟圖片
            if i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::O) {
                self.open_file_dialog();
            }

            // Ctrl + S / Ctrl + Shift + S: 儲存 / 另存新檔
            if i.modifiers.command && i.key_pressed(Key::S) {
                if i.modifiers.shift {
                    do_save_as = true;
                } else {
                    do_save = true;
                }
            }

            // Ctrl + Z / Ctrl + Shift + Z / Ctrl + Y: 復原 / 重做
            if i.modifiers.command && i.key_pressed(Key::Z) {
                if i.modifiers.shift {
                    do_redo = true;
                } else {
                    do_undo = true;
                }
            }
            if i.modifiers.command && i.key_pressed(Key::Y) {
                do_redo = true;
            }

            // Ctrl + D: 切換雙圖分割比對模式
            if i.modifiers.command && i.key_pressed(Key::D) {
                self.toggle_side_by_side();
            }

            // 圖片前後巡迴導覽 (左/右方向鍵, PageUp/PageDown, Home/End)
            if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::PageUp) {
                do_prev = true;
            }
            if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::PageDown) {
                do_next = true;
            }
            if i.key_pressed(Key::Home) {
                do_first = true;
            }
            if i.key_pressed(Key::End) {
                do_last = true;
            }

            // 視口縮放快捷鍵
            if i.key_pressed(Key::Num0) || i.key_pressed(Key::F) {
                self.apply_zoom_action(crate::panels::ZoomAction::Fit);
            }
            if i.key_pressed(Key::Num1) {
                self.apply_zoom_action(crate::panels::ZoomAction::Actual);
            }
            if i.key_pressed(Key::Equals) || i.key_pressed(Key::Plus) {
                self.apply_zoom_action(crate::panels::ZoomAction::In);
            }
            if i.key_pressed(Key::Minus) {
                self.apply_zoom_action(crate::panels::ZoomAction::Out);
            }

            // M / Shift + M: 局部放大鏡開關與倍率切換 (2x -> 4x -> 8x)
            if !i.modifiers.command && i.key_pressed(Key::M) {
                if i.modifiers.shift {
                    self.magnifier_zoom = match self.magnifier_zoom {
                        2 => 4,
                        4 => 8,
                        _ => 2,
                    };
                    self.status_notification = Some((
                        format!("🔍 局部放大鏡倍率切換為 {}x", self.magnifier_zoom),
                        std::time::Instant::now(),
                    ));
                } else {
                    self.magnifier_enabled = !self.magnifier_enabled;
                    if self.magnifier_enabled {
                        self.status_notification = Some((
                            format!(
                                "🔍 局部放大鏡已開啟 ({}x，Shift+M 可切換倍率)",
                                self.magnifier_zoom
                            ),
                            std::time::Instant::now(),
                        ));
                    } else {
                        self.status_notification =
                            Some(("🔍 局部放大鏡已關閉".to_string(), std::time::Instant::now()));
                    }
                }
            }
        });

        if do_undo {
            self.undo();
        }
        if do_redo {
            self.redo();
        }
        if do_save {
            self.save_current_image();
        }
        if do_save_as {
            self.save_image_as_dialog();
        }
        if do_prev {
            self.navigate_prev();
        }
        if do_next {
            self.navigate_next();
        }
        if do_first {
            self.navigate_to(0);
        }
        if do_last && let Some(last) = self.playlist.len().checked_sub(1) {
            self.navigate_to(last);
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.preferences.flush();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_preferences(ctx);
        self.poll_document_io();
        if self.io_needs_repaint() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        if self.leave_guard.close_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.request_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.has_pending_leave() || self.is_saving() {
            self.show_guarded_editor(ctx);
            return;
        }
        // 0. 更新視窗標題（帶未儲存星號 *）
        let title = match &self.session {
            Some(sess) => {
                let name = sess
                    .source_path()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("未命名圖片");
                let dirty_mark = if sess.is_dirty() { " *" } else { "" };
                let preview_mark = match self.document_status() {
                    crate::panels::DocumentStatus::Preview {
                        computing: true, ..
                    } => " · 預覽運算中",
                    crate::panels::DocumentStatus::Preview { .. } => " · 預覽未套用",
                    _ => "",
                };
                format!("{} - 圖片檢視器{}{}", name, dirty_mark, preview_mark)
            }
            None => "純 Rust 高效能圖片瀏覽器".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // 1. 處理全域檔案拖曳放入
        let dropped_file = ctx.input(|i| i.raw.dropped_files.first().and_then(|d| d.path.clone()));
        if let Some(path) = dropped_file
            && let Err(e) = self.open_file_path(path)
        {
            self.status_notification = Some((e, std::time::Instant::now()));
        }

        // 2. 處理鍵盤快捷鍵
        self.handle_shortcuts(ctx);

        // 3. 輪詢背景微調防抖計算結果
        if let Some(res) = self.debouncer.poll() {
            self.pending_adjustment = None;
            let params = self.slider_params;
            if let Some(sess) = &mut self.session {
                sess.set_preview_image(res.adjusted_image, params);
                ctx.request_repaint();
            }
        }
        if self.pending_adjustment.is_some() || self.debouncer.is_computing() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // 3.1 輪詢背景壓縮檔解壓結果 (非同步響應，UI 零卡頓)
        if self.poll_archive_loader() {
            ctx.request_repaint();
        }

        // 4. 接收背景縮圖結果並同步 GPU 主紋理與直方圖
        self.thumbnail_manager.poll_results(ctx);
        self.sync_texture(ctx);
        self.sync_histogram();
        if self.histogram_pending && self.show_side_panel {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        // 5. 頂部選單列
        let can_undo = self.session.as_ref().map(|s| s.can_undo()).unwrap_or(false);
        let can_redo = self.session.as_ref().map(|s| s.can_redo()).unwrap_or(false);

        let mut menu_state = crate::panels::MenuBarState {
            can_undo,
            can_redo,
            show_side_panel: &mut self.show_side_panel,
            show_filmstrip: &mut self.show_filmstrip,
            is_side_by_side: &mut self.comparison.is_active,
            sync_viewport: &mut self.comparison.sync_viewport,
            magnifier_enabled: &mut self.magnifier_enabled,
        };

        if let Some(action) = MenuBar::show(ctx, &mut menu_state) {
            match action {
                crate::panels::MenuAction::Open => self.open_file_dialog(),
                crate::panels::MenuAction::Save => {
                    self.save_current_image();
                }
                crate::panels::MenuAction::SaveAs => {
                    self.save_image_as_dialog();
                }
                crate::panels::MenuAction::Exit => {
                    if self.request_close() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                crate::panels::MenuAction::Undo => self.undo(),
                crate::panels::MenuAction::Redo => self.redo(),
                crate::panels::MenuAction::Reset100 => self.viewport.reset_actual_size(),
                crate::panels::MenuAction::ResetFit => self.viewport.reset_fit_window(),
                crate::panels::MenuAction::DuplicateFinder => {
                    let cur_dir = self
                        .session
                        .as_ref()
                        .and_then(|s| s.source_path())
                        .and_then(|p| p.parent())
                        .map(|p| p.to_path_buf());
                    self.duplicate_finder_state.open_with_dir(cur_dir);
                }
                crate::panels::MenuAction::About => self.show_about_dialog = true,
            }
        }

        if self.has_pending_leave() || self.is_saving() {
            self.show_guarded_editor(ctx);
            return;
        }

        // 6. 底部狀態列
        let (filename, dimensions) = match &self.session {
            Some(sess) => {
                let name = sess
                    .source_path()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str());
                let dims = (sess.active_image().width(), sess.active_image().height());
                (name, Some(dims))
            }
            None => (None, None),
        };

        let playlist_info = self
            .playlist
            .current_index()
            .map(|idx| (idx, self.playlist.len()));

        let status_msg = if self.is_loading_image() {
            Some("正在背景載入圖片…")
        } else if let Some((msg, ts)) = &self.status_notification {
            if ts.elapsed().as_secs() < 8 {
                Some(msg.as_str())
            } else {
                None
            }
        } else {
            None
        };

        let zoom_action = StatusBar::show(
            ctx,
            &crate::panels::StatusBarState {
                animated: self.animated_source,
                filename,
                dimensions,
                zoom: self.viewport.zoom,
                document: self.document_status(),
                playlist_info,
                notification: status_msg,
            },
        );
        if let Some(action) = zoom_action {
            self.apply_zoom_action(action);
        }

        // 7. 底部 Filmstrip 縮圖列
        let mut nav_to_idx = None;
        let mut do_prev = false;
        let mut do_next = false;

        FilmstripPanel::show(
            ctx,
            &mut self.show_filmstrip,
            &self.playlist,
            &mut self.thumbnail_manager,
            &mut |idx| nav_to_idx = Some(idx),
            &mut || do_prev = true,
            &mut || do_next = true,
        );

        if let Some(idx) = nav_to_idx {
            self.navigate_to(idx);
        }
        if do_prev {
            self.navigate_prev();
        }
        if do_next {
            self.navigate_next();
        }

        if self.has_pending_leave() {
            self.show_guarded_editor(ctx);
            return;
        }

        // 8. 側邊微調面板
        let has_image = self.session.is_some();
        let is_computing = self.is_preview_pending();
        let mut adjustment_action = None;

        let mut adj_state = AdjustmentPanelState {
            white_balance_open: &mut self.white_balance_open,
            fine_tune_open: &mut self.fine_tune_open,
            is_open: &mut self.show_side_panel,
            has_image,
            params: &mut self.slider_params,
            is_computing,
            histogram_data: self.cached_histogram.as_ref(),
            histogram_mode: &mut self.histogram_mode,
        };

        AdjustmentPanel::show(ctx, &mut adj_state, &mut adjustment_action);

        match adjustment_action {
            Some(crate::panels::AdjustmentAction::Change) => {
                if let Some(sess) = &mut self.session {
                    if self.slider_params.is_identity() {
                        self.debouncer.cancel_pending();
                        self.pending_adjustment = None;
                        sess.cancel_adjustments();
                    } else {
                        let base = sess.committed_image();
                        self.pending_adjustment =
                            Some(self.debouncer.request(base, self.slider_params));
                    }
                }
            }
            Some(crate::panels::AdjustmentAction::Reset)
            | Some(crate::panels::AdjustmentAction::Cancel) => {
                self.debouncer.cancel_pending();
                self.pending_adjustment = None;
                self.slider_params = AdjustmentParams::default();
                if let Some(sess) = &mut self.session {
                    sess.cancel_adjustments();
                }
            }
            Some(crate::panels::AdjustmentAction::Apply) => {
                self.apply_current_adjustments();
            }
            None => {}
        }

        // 9. 中央 GPU 加速影像畫布 (支援單圖或雙圖分割對比)
        CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let dims = self.session.as_ref().map(|s| {
                    let img = s.active_image();
                    (img.width(), img.height())
                });

                let (left_tex, left_dims) = match self.comparison.source {
                    ComparisonSource::Original => {
                        let l_dims = self.session.as_ref().map(|s| {
                            let img = s.original_image();
                            (img.width(), img.height())
                        });
                        (self.original_texture.as_ref(), l_dims)
                    }
                    ComparisonSource::Reference => {
                        let l_dims = self
                            .reference_image
                            .as_ref()
                            .map(|img| (img.width(), img.height()));
                        (self.reference_texture.as_ref(), l_dims)
                    }
                };

                let active_img_ref = self.session.as_ref().map(|s| s.active_image());
                let mut canvas_state = CanvasState {
                    texture: self.texture.as_ref(),
                    image_dims: dims,
                    viewport: &mut self.viewport,
                    left_texture: left_tex,
                    left_dims,
                    left_viewport: &mut self.left_viewport,
                    comparison: &mut self.comparison,
                    image_data: active_img_ref.as_deref(),
                    magnifier_enabled: self.magnifier_enabled,
                    magnifier_zoom: self.magnifier_zoom,
                };

                let mut comparison_action = None;
                CanvasView::show(ui, &mut canvas_state, &mut comparison_action);

                match comparison_action {
                    Some(ComparisonAction::ToggleSync) => {
                        self.comparison.sync_viewport = !self.comparison.sync_viewport;
                        if self.comparison.sync_viewport {
                            self.left_viewport = self.viewport;
                        }
                    }
                    Some(ComparisonAction::SetSource(src)) => {
                        self.comparison.source = src;
                    }
                    Some(ComparisonAction::OpenReferenceFile) => {
                        self.open_reference_image_dialog();
                    }
                    Some(ComparisonAction::CloseComparison) => {
                        self.comparison.is_active = false;
                    }
                    None => {}
                }
            });

        // 10. 關於 Rust Image Viewer 彈窗
        AboutDialog::show(ctx, &mut self.show_about_dialog);

        // 11. 重複圖片檢查器彈窗
        if let Some(target_path) =
            DuplicateFinderDialog::show(ctx, &mut self.duplicate_finder_state)
        {
            let _ = self.open_image_path(target_path);
        }

        // 12. 壓縮檔密碼輸入彈窗
        if self.show_password_dialog {
            if let Some(archive_path) = self.pending_archive_path.clone() {
                let mut is_open = self.show_password_dialog;
                let action = crate::panels::ArchivePasswordDialog::new(
                    &archive_path,
                    &mut self.archive_password_input,
                    self.archive_password_error.as_deref(),
                    &mut self.archive_show_password,
                    &mut is_open,
                )
                .show(ctx);

                self.show_password_dialog = is_open;

                match action {
                    crate::panels::ArchivePasswordAction::Confirm(pwd) => {
                        let _ = self.open_archive_with_password(archive_path, &pwd);
                    }
                    crate::panels::ArchivePasswordAction::Cancel => {
                        self.cancel_archive_loading();
                    }
                    crate::panels::ArchivePasswordAction::None => {}
                }
            } else {
                self.show_password_dialog = false;
            }
        }

        // 13. 壓縮檔背景解壓中 HUD 提示視窗（方案 1：即時進度條與動態回饋）
        if self.archive_loader.is_extracting()
            || self.latest_archive_req_id > self.received_archive_req_id
        {
            let loading_name = self
                .archive_loader
                .loading_file_name()
                .unwrap_or_else(|| "壓縮檔".to_string());

            let progress_opt = self.archive_loader.current_progress();

            egui::Window::new("📦 正在安全解壓縮")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .fixed_size(egui::vec2(380.0, 136.0))
                .frame(
                    egui::Frame::window(&ctx.style())
                        .fill(egui::Color32::from_rgb(26, 26, 32))
                        .stroke(egui::Stroke::new(
                            1.5_f32,
                            crate::theme::ThemeColors::ACCENT,
                        ))
                        .rounding(egui::Rounding::same(8.0))
                        .inner_margin(egui::Margin::same(16.0)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(format!("正在解壓 '{}'...", loading_name))
                                    .color(egui::Color32::WHITE)
                                    .strong(),
                            );
                        });

                        ui.add_space(10.0);

                        if let Some(prog) = progress_opt {
                            if prog.total_entries > 0 {
                                let ratio = (prog.current_entry_index as f32
                                    / prog.total_entries as f32)
                                    .clamp(0.0, 1.0);
                                ui.add(
                                    egui::ProgressBar::new(ratio)
                                        .show_percentage()
                                        .animate(true)
                                        .text(format!(
                                            "{}/{} 檔案",
                                            prog.current_entry_index, prog.total_entries
                                        )),
                                );
                            } else {
                                ui.add(
                                    egui::ProgressBar::new(1.0)
                                        .animate(true)
                                        .text("分析與解壓串流中..."),
                                );
                            }

                            if !prog.current_entry.is_empty() {
                                ui.add_space(6.0);
                                let char_count = prog.current_entry.chars().count();
                                let entry_display = if char_count > 32 {
                                    let skip_count = char_count.saturating_sub(30);
                                    let suffix: String =
                                        prog.current_entry.chars().skip(skip_count).collect();
                                    format!("...{}", suffix)
                                } else {
                                    prog.current_entry.clone()
                                };
                                ui.label(
                                    egui::RichText::new(format!("檔案: {}", entry_display))
                                        .color(egui::Color32::LIGHT_GRAY)
                                        .size(11.5),
                                );
                            }
                        } else {
                            ui.add(
                                egui::ProgressBar::new(1.0)
                                    .animate(true)
                                    .text("準備解壓縮引擎..."),
                            );
                        }

                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("已移至背景非同步執行，UI 保持流暢絕不凍結")
                                .color(crate::theme::ThemeColors::TEXT_MUTED)
                                .size(11.5),
                        );
                    });
                });
            ctx.request_repaint();
        }
        self.show_leave_dialog(ctx);
        self.update_preferences(ctx);
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn all_display_sources_fit_gpu_limit_and_save_full_resolution() {
        use std::time::{Duration, Instant};
        let mut app = ImageViewerApp {
            session: Some(EditingSession::new(image::RgbaImage::new(2000, 4), None)),
            reference_image: Some(Arc::new(image::RgbaImage::new(4, 2000))),
            ..Default::default()
        };
        app.comparison.is_active = true;
        let ctx = egui::Context::default();
        for source in [ComparisonSource::Original, ComparisonSource::Reference] {
            app.comparison.source = source;
            let start = Instant::now();
            loop {
                let _ = ctx.run(
                    egui::RawInput {
                        max_texture_side: Some(1024),
                        ..Default::default()
                    },
                    |ctx| app.sync_texture(ctx),
                );
                let left = match source {
                    ComparisonSource::Original => app.original_texture.as_ref(),
                    ComparisonSource::Reference => app.reference_texture.as_ref(),
                };
                if let (Some(main), Some(left)) = (&app.texture, left) {
                    assert_eq!(main.size(), [1024, 2]);
                    assert_eq!(
                        left.size(),
                        if source == ComparisonSource::Original {
                            [1024, 2]
                        } else {
                            [2, 1024]
                        }
                    );
                    break;
                }
                assert!(start.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("full.png");
        assert!(app.save_image_to(&path));
        assert!(app.wait_document_io(Duration::from_secs(5)));
        assert_eq!(image::image_dimensions(path).unwrap(), (2000, 4));
        assert_eq!(
            app.session.as_ref().unwrap().original_image().dimensions(),
            (2000, 4)
        );
    }

    #[test]
    fn queued_preview_is_reported_as_computing_without_marking_the_base_dirty() {
        let app = ImageViewerApp {
            session: Some(EditingSession::new(image::RgbaImage::new(1, 1), None)),
            pending_adjustment: Some(1),
            ..Default::default()
        };
        assert_eq!(
            app.document_status(),
            crate::panels::DocumentStatus::Preview {
                committed_dirty: false,
                computing: true
            }
        );
    }

    #[test]
    fn editing_a_number_does_not_trigger_zoom_or_navigation_shortcuts() {
        let mut app = ImageViewerApp {
            session: Some(EditingSession::new(image::RgbaImage::new(1, 1), None)),
            ..Default::default()
        };
        app.playlist
            .set_custom_list(vec!["a.png".into(), "b.png".into()]);
        app.viewport.zoom = 0.5;
        let ctx = egui::Context::default();
        let mut text = String::new();
        let _ = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.text_edit_singleline(&mut text).request_focus();
            });
        });
        let events = [Key::Num1, Key::ArrowRight]
            .into_iter()
            .map(|key| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            })
            .collect();
        let _ = ctx.run(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ctx| app.handle_shortcuts(ctx),
        );
        assert_eq!(app.viewport.zoom, 0.5);
        assert!(!app.is_loading_image());
        assert_eq!(app.playlist.current_index(), Some(0));
    }
}
