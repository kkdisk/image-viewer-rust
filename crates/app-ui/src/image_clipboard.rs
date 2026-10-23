//! 以已套用影像快照進行單項背景剪貼簿寫入。
use crate::app::ImageViewerApp;
use image::RgbaImage;
use std::sync::{Arc, mpsc};

const MAX_CLIPBOARD_BYTES: usize = 128 * 1024 * 1024;
fn validate_size(width: u32, height: u32, bytes: usize) -> Result<(), &'static str> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|size| size.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(bytes) {
        return Err("圖片尺寸無效，無法複製");
    }
    if bytes > MAX_CLIPBOARD_BYTES {
        return Err("圖片超過剪貼簿 128 MiB 像素上限，請先改尺寸後再複製");
    }
    Ok(())
}
struct PendingCopy {
    receiver: mpsc::Receiver<Result<(), String>>,
    label: String,
}
#[derive(Default)]
pub(crate) struct ImageClipboard {
    pending: Option<PendingCopy>,
}
impl ImageClipboard {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    fn start_with(
        &mut self,
        source: Arc<RgbaImage>,
        label: String,
        write: impl FnOnce(&RgbaImage) -> Result<(), String> + Send + 'static,
    ) -> Result<(), String> {
        if self.is_pending() {
            return Err("正在複製圖片，請等待完成後再試".into());
        }
        let (tx, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("image-clipboard".into())
            .spawn(move || {
                let _ = tx.send(write(&source));
            })
            .map_err(|error| format!("無法啟動圖片複製工作：{error}"))?;
        self.pending = Some(PendingCopy { receiver, label });
        Ok(())
    }
    fn poll(&mut self) -> Option<(String, Result<(), String>)> {
        let pending = self.pending.as_ref()?;
        let result = match pending.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err("圖片複製工作未正常完成，請重試".into()),
        };
        Some((self.pending.take()?.label, result))
    }
}

#[cfg(windows)]
fn write_system_image(image: &RgbaImage) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width: image.width() as usize,
            height: image.height() as usize,
            bytes: std::borrow::Cow::Borrowed(image.as_raw()),
        })
        .map_err(|error| error.to_string())
}
#[cfg(not(windows))]
fn write_system_image(_: &RgbaImage) -> Result<(), String> {
    Err("此版本的圖片剪貼簿功能僅支援 Windows".into())
}

impl ImageViewerApp {
    fn clipboard_source(&self) -> Result<Arc<RgbaImage>, &'static str> {
        let session = self.session.as_ref().ok_or("請先開啟圖片再複製")?;
        if self.image_clipboard.is_pending() {
            return Err("正在複製圖片，請等待完成後再試");
        }
        if self.is_document_busy()
            || self.is_loading_image()
            || self.is_transforming()
            || self.is_recycling()
            || self.has_pending_leave()
            || self.file_actions.confirming_delete
        {
            return Err("圖片正在處理或等候確認，請稍後再複製");
        }
        if self.geometry.is_some()
            || self.is_preview_pending()
            || session.has_active_preview()
            || !self.slider_params.is_identity()
        {
            return Err("請先套用或取消預覽，再複製圖片");
        }
        let source = session.committed_image();
        validate_size(source.width(), source.height(), source.as_raw().len())?;
        Ok(source)
    }
    pub(crate) fn can_copy_image(&self) -> bool {
        self.clipboard_source().is_ok()
    }
    pub(crate) fn copy_image(&mut self) {
        let result = self
            .clipboard_source()
            .map_err(str::to_owned)
            .and_then(|source| {
                let name = self
                    .session
                    .as_ref()
                    .and_then(|s| s.source_path())
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "未命名圖片".into());
                let label = format!("{name}（{} × {} px）", source.width(), source.height());
                self.image_clipboard
                    .start_with(source, label, write_system_image)
            });
        self.status_notification = Some((
            match result {
                Ok(()) => "正在背景複製圖片，完成後即可貼上…".into(),
                Err(error) => error,
            },
            std::time::Instant::now(),
        ));
    }
    pub(crate) fn poll_image_clipboard(&mut self, ctx: &egui::Context) {
        if let Some((label, result)) = self.image_clipboard.poll() {
            self.status_notification = Some((
                match result {
                    Ok(()) => format!("已複製圖片：{label}"),
                    Err(error) => format!("複製圖片失敗：{label}；{error}；請重試"),
                },
                std::time::Instant::now(),
            ));
        }
        if self.image_clipboard.is_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(app: &mut ImageViewerApp) {
        let ctx = egui::Context::default();
        let start = Instant::now();
        while app.image_clipboard.is_pending() {
            app.poll_image_clipboard(&ctx);
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn 完整快照保留透明像素且不因後續編輯改變並限制單項工作() {
        let pixels = RgbaImage::from_raw(
            2,
            2,
            vec![10, 20, 30, 0, 40, 50, 60, 127, 70, 80, 90, 255, 1, 2, 3, 4],
        )
        .unwrap();
        let mut app = ImageViewerApp::default();
        app.session = Some(viewer_state::EditingSession::new(
            pixels.clone(),
            Some("原圖.png".into()),
        ));
        app.viewport.zoom = 0.1;
        let source = app.clipboard_source().unwrap();
        assert!(Arc::ptr_eq(
            &source,
            &app.session.as_ref().unwrap().committed_image()
        ));
        let (release, gate) = mpsc::sync_channel(1);
        let (tx, result) = mpsc::sync_channel(1);
        app.image_clipboard
            .start_with(
                source.clone(),
                "原圖.png（2 × 2 px）".into(),
                move |image| {
                    gate.recv_timeout(Duration::from_secs(5)).unwrap();
                    tx.send(image.clone()).unwrap();
                    Ok(())
                },
            )
            .unwrap();
        assert!(
            app.image_clipboard
                .start_with(source, "重複".into(), |_| panic!("重複請求不應執行"))
                .is_err()
        );
        assert!(!app.request_close());
        app.session
            .as_mut()
            .unwrap()
            .push_action(RgbaImage::new(1, 1), "後續編輯");
        let identity = app.session.as_ref().unwrap().identity();
        release.send(()).unwrap();
        wait(&mut app);
        assert_eq!(result.recv_timeout(Duration::from_secs(5)).unwrap(), pixels);
        assert_eq!(app.session.as_ref().unwrap().identity(), identity);
        assert!(
            app.status_notification
                .as_ref()
                .unwrap()
                .0
                .contains("已複製圖片：原圖.png")
        );
        assert_eq!(app.viewport.zoom, 0.1);
    }
    #[test]
    fn 拒絕未載入預覽與不合理大小並回報寫入失敗() {
        assert!(validate_size(0, 1, 0).is_err());
        assert!(validate_size(1, 1, 3).is_err());
        assert!(validate_size(8192, 4096, MAX_CLIPBOARD_BYTES).is_ok());
        assert!(validate_size(8193, 4096, 8193 * 4096 * 4).is_err());
        let mut app = ImageViewerApp::default();
        assert!(!app.can_copy_image());
        app.session = Some(viewer_state::EditingSession::new(
            RgbaImage::new(1, 1),
            None,
        ));
        app.slider_params.brightness = 1.1;
        assert!(!app.can_copy_image());
        app.copy_image();
        assert!(!app.image_clipboard.is_pending());
        assert!(
            app.status_notification
                .as_ref()
                .unwrap()
                .0
                .contains("套用或取消預覽")
        );
        app.slider_params = Default::default();
        let identity = app.session.as_ref().unwrap().identity();
        let source = app.clipboard_source().unwrap();
        app.image_clipboard
            .start_with(source, "失敗測試".into(), |_| Err("剪貼簿被占用".into()))
            .unwrap();
        wait(&mut app);
        assert!(
            app.status_notification
                .as_ref()
                .unwrap()
                .0
                .contains("複製圖片失敗：失敗測試")
        );
        assert_eq!(app.session.as_ref().unwrap().identity(), identity);
        assert!(!app.has_unsaved_changes());
    }
}
