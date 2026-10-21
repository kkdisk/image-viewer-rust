//! eframe::App 主桌面應用邏輯

use std::path::PathBuf;
use std::sync::Arc;
use egui::{CentralPanel, ColorImage, Key, TextureHandle, TextureOptions};
use viewer_state::{EditingSession, ViewportState};

use crate::canvas::CanvasView;
use crate::panels::{AdjustmentPanel, FilmstripPanel, MenuBar, StatusBar};
use crate::theme::apply_theme;

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
    /// GPU 影像紋理快取
    pub texture: Option<TextureHandle>,
    /// 目前上傳到 GPU 紋理的底層影像指標，用於避免重複上傳
    last_texture_ptr: usize,
}

impl ImageViewerApp {
    /// 建立應用實例
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 套用自訂現代暗色主題與微軟正黑體字型
        apply_theme(&cc.egui_ctx);

        Self {
            session: None,
            viewport: ViewportState::new(),
            show_side_panel: true,
            show_filmstrip: true,
            texture: None,
            last_texture_ptr: 0,
        }
    }

    /// 開啟指定路徑之影像
    pub fn open_image_path(&mut self, path: PathBuf) -> Result<(), core_image::ImageError> {
        let dyn_img = core_image::load_image(&path)?;
        let rgba = dyn_img.to_rgba8();
        self.session = Some(EditingSession::new(rgba, Some(path)));
        self.viewport.reset_fit_window();
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

    /// 更新 GPU 影像紋理
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
        } else {
            self.texture = None;
            self.last_texture_ptr = 0;
        }
    }

    /// 處理鍵盤快捷鍵
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Ctrl + O: 開啟圖片
            if i.modifiers.command && i.key_pressed(Key::O) {
                self.open_file_dialog();
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
    }
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 1. 處理全域檔案拖曳放入
        ctx.input(|i| {
            if let Some(path) = i.raw.dropped_files.first().and_then(|d| d.path.as_ref()) {
                let _ = self.open_image_path(path.clone());
            }
        });

        // 2. 處理鍵盤快捷鍵
        self.handle_shortcuts(ctx);

        // 3. 同步 GPU 紋理
        self.sync_texture(ctx);

        // 4. 頂部選單列回呼
        let mut do_open = false;
        let mut do_reset_100 = false;
        let mut do_reset_fit = false;

        let mut on_open = || {
            do_open = true;
        };

        let mut on_save = || {
            if let Some(session) = &mut self.session {
                let source = session.source_path().map(|p| p.to_path_buf());
                if let Some(path) = source {
                    let dyn_img = image::DynamicImage::ImageRgba8((*session.committed_image()).clone());
                    if core_image::save_image_atomic(&dyn_img, path).is_ok() {
                        session.mark_saved();
                    }
                }
            }
        };

        MenuBar::show(
            ctx,
            &mut on_open,
            &mut on_save,
            &mut self.show_side_panel,
            &mut self.show_filmstrip,
            &mut do_reset_100,
            &mut do_reset_fit,
        );

        if do_open {
            self.open_file_dialog();
        }
        if do_reset_100 {
            self.viewport.reset_actual_size();
        }
        if do_reset_fit {
            self.viewport.reset_fit_window();
        }

        // 5. 底部狀態列
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

        StatusBar::show(
            ctx,
            filename,
            dimensions,
            self.viewport.zoom,
            is_dirty,
        );

        // 6. 底部 Filmstrip 容器
        FilmstripPanel::show(ctx, &mut self.show_filmstrip);

        // 7. 側邊調整面板
        AdjustmentPanel::show(ctx, &mut self.show_side_panel);

        // 8. 中央 GPU 加速影像畫布
        CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let dims = self.session.as_ref().map(|s| {
                    let img = s.active_image();
                    (img.width(), img.height())
                });
                CanvasView::show(ui, self.texture.as_ref(), dims, &mut self.viewport);
            });
    }
}
