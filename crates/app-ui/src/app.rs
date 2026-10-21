//! eframe::App 主桌面應用邏輯

use std::path::PathBuf;
use egui::CentralPanel;
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
}

impl ImageViewerApp {
    /// 建立應用實例
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // 套用自訂現代暗色主題
        apply_theme(&cc.egui_ctx);

        Self {
            session: None,
            viewport: ViewportState::new(),
            show_side_panel: true,
            show_filmstrip: true,
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
}

impl eframe::App for ImageViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 處理全域拖曳檔案放入事件
        ctx.input(|i| {
            if let Some(path) = i.raw.dropped_files.first().and_then(|d| d.path.as_ref()) {
                let _ = self.open_image_path(path.clone());
            }
        });

        // 1. 頂部選單列
        let mut on_open = || {
            // 在沒有外部對話框 crate 時，可透過控制台或後續原生對話框整合
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
        );

        // 2. 底部狀態列
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

        // 3. 底部 Filmstrip 容器
        FilmstripPanel::show(ctx, &mut self.show_filmstrip);

        // 4. 側邊調整面板
        AdjustmentPanel::show(ctx, &mut self.show_side_panel);

        // 5. 中央畫布主區域
        CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let has_img = self.session.is_some();
                CanvasView::show(ui, has_img, filename);
            });
    }
}
