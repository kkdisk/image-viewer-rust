//! eframe::App 主桌面應用邏輯

use std::path::PathBuf;
use std::sync::Arc;
use core_image::AdjustmentParams;
use egui::{CentralPanel, ColorImage, Key, TextureHandle, TextureOptions};
use viewer_state::{EditingSession, ViewportState};

use crate::canvas::{CanvasState, CanvasView, ComparisonAction, ComparisonConfig, ComparisonSource};
use crate::debounce_queue::AdjustmentDebouncer;
use crate::panels::{AboutDialog, AdjustmentPanel, FilmstripPanel, MenuBar, StatusBar};
use crate::playlist::ImagePlaylist;
use crate::theme::apply_theme;
use crate::thumbnail_loader::ThumbnailManager;

/// 主圖片瀏覽與編輯桌面應用狀態
pub struct ImageViewerApp {
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
    /// GPU 影像紋理快取
    pub texture: Option<TextureHandle>,
    /// 目前上傳到 GPU 紋理的底層影像指標，用於避免重複上傳
    last_texture_ptr: usize,
    /// 目錄圖片清單與巡迴導覽管理器
    pub playlist: ImagePlaylist,
    /// 非同步縮圖生成與快取管理器
    pub thumbnail_manager: ThumbnailManager,
    /// 側邊微調面板目前滑桿參數
    pub slider_params: AdjustmentParams,
    /// 像素微調防抖背景運算排程器
    pub debouncer: AdjustmentDebouncer,
    /// 雙圖比對視圖配置
    pub comparison: ComparisonConfig,
    /// 雙圖比對時左側獨立視口（當未開啟同步時使用）
    pub left_viewport: ViewportState,
    /// 原始底圖 GPU 紋理（用於原圖比對）
    pub original_texture: Option<TextureHandle>,
    /// 原始底圖底層指標
    last_original_ptr: usize,
    /// 外部參考圖 RGBA 快照
    pub reference_image: Option<Arc<image::RgbaImage>>,
    /// 外部參考圖 GPU 紋理
    pub reference_texture: Option<TextureHandle>,
}

impl Default for ImageViewerApp {
    fn default() -> Self {
        Self {
            session: None,
            viewport: ViewportState::new(),
            show_side_panel: true,
            show_filmstrip: true,
            show_about_dialog: false,
            texture: None,
            last_texture_ptr: 0,
            playlist: ImagePlaylist::new(),
            thumbnail_manager: ThumbnailManager::new(),
            slider_params: AdjustmentParams::default(),
            debouncer: AdjustmentDebouncer::new(),
            comparison: ComparisonConfig::default(),
            left_viewport: ViewportState::new(),
            original_texture: None,
            last_original_ptr: 0,
            reference_image: None,
            reference_texture: None,
        }
    }
}

impl ImageViewerApp {
    /// 建立應用實例
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 套用自訂現代暗色主題與微軟正黑體字型
        apply_theme(&cc.egui_ctx);
        Self::default()
    }

    /// 開啟指定路徑之影像，並自動掃描同目錄圖片加入清單
    pub fn open_image_path(&mut self, path: PathBuf) -> Result<(), core_image::ImageError> {
        let dyn_img = core_image::load_image(&path)?;
        let rgba = dyn_img.to_rgba8();

        // 檢查是否更換了目錄，若是則重新掃描
        let current_dir = path.parent();
        let prev_dir = self.session.as_ref().and_then(|s| s.source_path()).and_then(|p| p.parent());

        if current_dir != prev_dir {
            self.thumbnail_manager.clear();
        }
        self.playlist.scan_from_file(&path);

        self.session = Some(EditingSession::new(rgba, Some(path)));
        self.slider_params = AdjustmentParams::default();
        self.viewport.reset_fit_window();
        self.left_viewport = self.viewport;
        self.original_texture = None;
        self.last_original_ptr = 0;
        self.reference_texture = None;
        self.reference_image = None;
        self.comparison.reference_path = None;
        Ok(())
    }

    /// 彈出原生系統檔案選取對話框
    pub fn open_file_dialog(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter("圖片檔案 (*.png, *.jpg, *.jpeg, *.bmp, *.webp, *.tiff, *.gif)", &["png", "jpg", "jpeg", "bmp", "webp", "tiff", "gif"])
            .add_filter("所有檔案 (*.*)", &["*"])
            .pick_file();

        if let Some(path) = file {
            let _ = self.open_image_path(path);
        }
    }

    /// 開啟外部參考圖檔案對話框
    pub fn open_reference_image_dialog(&mut self) {
        let file = rfd::FileDialog::new()
            .add_filter("圖片檔案 (*.png, *.jpg, *.jpeg, *.bmp, *.webp, *.tiff, *.gif)", &["png", "jpg", "jpeg", "bmp", "webp", "tiff", "gif"])
            .add_filter("所有檔案 (*.*)", &["*"])
            .pick_file();

        if let Some(path) = file
            && let Ok(dyn_img) = core_image::load_image(&path)
        {
            let rgba = dyn_img.to_rgba8();
            self.reference_image = Some(Arc::new(rgba));
            self.reference_texture = None;
            self.comparison.reference_path = Some(path);
            self.comparison.source = ComparisonSource::Reference;
            self.comparison.is_active = true;
        }
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
        if let Some(path) = self.playlist.prev() {
            let _ = self.open_image_path(path);
        }
    }

    /// 切換至下一張圖片
    pub fn navigate_next(&mut self) {
        if let Some(path) = self.playlist.next() {
            let _ = self.open_image_path(path);
        }
    }

    /// 切換至指定索引之圖片
    pub fn navigate_to(&mut self, index: usize) {
        if let Some(path) = self.playlist.jump_to(index) {
            let _ = self.open_image_path(path);
        }
    }

    /// 更新 GPU 影像紋理（主視口與雙圖左側視口）
    fn sync_texture(&mut self, ctx: &egui::Context) {
        if let Some(sess) = &self.session {
            let active = sess.active_image();
            let current_ptr = Arc::as_ptr(&active) as usize;

            if self.texture.is_none() || self.last_texture_ptr != current_ptr {
                let size = [active.width() as usize, active.height() as usize];
                let color_image = ColorImage::from_rgba_unmultiplied(size, active.as_raw());

                // 採用雙線性過濾 LINEAR，確保縮放時平滑流暢
                let handle = ctx.load_texture(
                    "active_viewport_image",
                    color_image,
                    TextureOptions::LINEAR,
                );
                self.texture = Some(handle);
                self.last_texture_ptr = current_ptr;
            }

            // 若雙圖分割對比啟用中，更新左側紋理
            if self.comparison.is_active {
                match self.comparison.source {
                    ComparisonSource::Original => {
                        let orig = sess.original_image();
                        let orig_ptr = Arc::as_ptr(&orig) as usize;
                        if self.original_texture.is_none() || self.last_original_ptr != orig_ptr {
                            let size = [orig.width() as usize, orig.height() as usize];
                            let color_image = ColorImage::from_rgba_unmultiplied(size, orig.as_raw());
                            let handle = ctx.load_texture(
                                "original_viewport_image",
                                color_image,
                                TextureOptions::LINEAR,
                            );
                            self.original_texture = Some(handle);
                            self.last_original_ptr = orig_ptr;
                        }
                    }
                    ComparisonSource::Reference => {
                        if let Some(ref_img) = &self.reference_image {
                            if self.reference_texture.is_none() {
                                let size = [ref_img.width() as usize, ref_img.height() as usize];
                                let color_image = ColorImage::from_rgba_unmultiplied(size, ref_img.as_raw());
                                let handle = ctx.load_texture(
                                    "reference_viewport_image",
                                    color_image,
                                    TextureOptions::LINEAR,
                                );
                                self.reference_texture = Some(handle);
                            }
                        } else {
                            self.reference_texture = None;
                        }
                    }
                }
            }
        } else {
            self.texture = None;
            self.last_texture_ptr = 0;
            self.original_texture = None;
            self.last_original_ptr = 0;
        }
    }

    /// 復原上一動作
    pub fn undo(&mut self) {
        if let Some(session) = &mut self.session
            && session.undo()
        {
            self.slider_params = AdjustmentParams::default();
        }
    }

    /// 重做下一動作
    pub fn redo(&mut self) {
        if let Some(session) = &mut self.session
            && session.redo()
        {
            self.slider_params = AdjustmentParams::default();
        }
    }

    /// 原地原子儲存已提交之影像
    pub fn save_current_image(&mut self) -> bool {
        if let Some(session) = &mut self.session {
            if let Some(path) = session.source_path().map(|p| p.to_path_buf()) {
                let dyn_img = image::DynamicImage::ImageRgba8((*session.committed_image()).clone());
                if core_image::save_image_atomic(&dyn_img, &path).is_ok() {
                    session.mark_saved();
                    return true;
                }
            } else {
                // 若無來源路徑則轉為另存新檔
                self.save_image_as_dialog();
                return true;
            }
        }
        false
    }

    /// 彈出原生對話框另存新檔
    pub fn save_image_as_dialog(&mut self) {
        if let Some(session) = &mut self.session {
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
                let dyn_img = image::DynamicImage::ImageRgba8((*session.committed_image()).clone());
                if core_image::save_image_atomic(&dyn_img, &target_path).is_ok() {
                    session.set_source_path(target_path.clone());
                    session.mark_saved();
                    // 若新檔案加入目錄，更新 playlist
                    self.playlist.scan_from_file(&target_path);
                }
            }
        }
    }

    /// 處理鍵盤快捷鍵
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
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
                self.viewport.reset_fit_window();
            }
            if i.key_pressed(Key::Num1) {
                self.viewport.reset_actual_size();
            }
            if i.key_pressed(Key::Equals) || i.key_pressed(Key::Plus) {
                self.viewport.zoom = (self.viewport.zoom * 1.2).clamp(0.02, 50.0);
                self.viewport.auto_fit = false;
            }
            if i.key_pressed(Key::Minus) {
                self.viewport.zoom = (self.viewport.zoom / 1.2).clamp(0.02, 50.0);
                self.viewport.auto_fit = false;
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
            let first_path = self.playlist.first();
            if let Some(path) = first_path {
                let _ = self.open_image_path(path);
            }
        }
        if do_last {
            let last_path = self.playlist.last();
            if let Some(path) = last_path {
                let _ = self.open_image_path(path);
            }
        }
    }
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 0. 更新視窗標題（帶未儲存星號 *）
        let title = match &self.session {
            Some(sess) => {
                let name = sess
                    .source_path()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("未命名圖片");
                let dirty_mark = if sess.is_dirty() { " *" } else { "" };
                format!("{} - 圖片檢視器{}", name, dirty_mark)
            }
            None => "純 Rust 高效能圖片瀏覽器".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        // 1. 處理全域檔案拖曳放入
        ctx.input(|i| {
            if let Some(path) = i.raw.dropped_files.first().and_then(|d| d.path.as_ref()) {
                let _ = self.open_image_path(path.clone());
            }
        });

        // 2. 處理鍵盤快捷鍵
        self.handle_shortcuts(ctx);

        // 3. 輪詢背景微調防抖計算結果
        if let Some(res) = self.debouncer.poll() {
            let params = self.slider_params;
            if let Some(sess) = &mut self.session {
                sess.set_preview_image(res.adjusted_image, params);
                ctx.request_repaint();
            }
        }

        // 4. 接收背景縮圖結果並同步 GPU 主紋理
        self.thumbnail_manager.poll_results(ctx);
        self.sync_texture(ctx);

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
        };

        if let Some(action) = MenuBar::show(ctx, &mut menu_state) {
            match action {
                crate::panels::MenuAction::Open => self.open_file_dialog(),
                crate::panels::MenuAction::Save => {
                    self.save_current_image();
                }
                crate::panels::MenuAction::SaveAs => self.save_image_as_dialog(),
                crate::panels::MenuAction::Undo => self.undo(),
                crate::panels::MenuAction::Redo => self.redo(),
                crate::panels::MenuAction::Reset100 => self.viewport.reset_actual_size(),
                crate::panels::MenuAction::ResetFit => self.viewport.reset_fit_window(),
                crate::panels::MenuAction::About => self.show_about_dialog = true,
            }
        }

        // 6. 底部狀態列
        let (filename, dimensions, is_dirty) = match &self.session {
            Some(sess) => {
                let name = sess
                    .source_path()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str());
                let dims = (sess.active_image().width(), sess.active_image().height());
                (name, Some(dims), sess.is_dirty())
            }
            None => (None, None, false),
        };

        let playlist_info = self.playlist.current_index().map(|idx| (idx, self.playlist.len()));

        StatusBar::show(
            ctx,
            filename,
            dimensions,
            self.viewport.zoom,
            is_dirty,
            playlist_info,
        );

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

        // 8. 側邊微調面板
        let has_image = self.session.is_some();
        let is_computing = self.debouncer.is_computing();
        let mut adjustment_action = None;

        AdjustmentPanel::show(
            ctx,
            &mut self.show_side_panel,
            has_image,
            &mut self.slider_params,
            is_computing,
            &mut adjustment_action,
        );

        match adjustment_action {
            Some(crate::panels::AdjustmentAction::Change) => {
                if let Some(sess) = &mut self.session {
                    if self.slider_params.is_identity() {
                        sess.cancel_adjustments();
                    } else {
                        let base = sess.committed_image();
                        self.debouncer.request(base, self.slider_params);
                    }
                }
            }
            Some(crate::panels::AdjustmentAction::Reset)
            | Some(crate::panels::AdjustmentAction::Cancel) => {
                self.slider_params = AdjustmentParams::default();
                if let Some(sess) = &mut self.session {
                    sess.cancel_adjustments();
                }
            }
            Some(crate::panels::AdjustmentAction::Apply) => {
                if let Some(sess) = &mut self.session {
                    sess.apply_adjustments();
                }
                self.slider_params = AdjustmentParams::default();
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
                        let l_dims = self.reference_image.as_ref().map(|img| (img.width(), img.height()));
                        (self.reference_texture.as_ref(), l_dims)
                    }
                };

                let mut canvas_state = CanvasState {
                    texture: self.texture.as_ref(),
                    image_dims: dims,
                    viewport: &mut self.viewport,
                    left_texture: left_tex,
                    left_dims,
                    left_viewport: &mut self.left_viewport,
                    comparison: &mut self.comparison,
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
    }
}
