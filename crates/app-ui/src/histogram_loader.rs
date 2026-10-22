//! 直方圖背景工作：等待中的影像僅保留最新一張。

use core_image::HistogramData;
use image::RgbaImage;
use std::sync::{Arc, Mutex, mpsc};

pub struct HistogramResult {
    pub image: Arc<RgbaImage>,
    pub histogram: HistogramData,
}

pub struct HistogramLoader {
    pending: Arc<Mutex<Option<Arc<RgbaImage>>>>,
    wake: mpsc::SyncSender<()>,
    results: mpsc::Receiver<HistogramResult>,
}

impl Default for HistogramLoader {
    fn default() -> Self {
        let pending = Arc::new(Mutex::new(None::<Arc<RgbaImage>>));
        let worker_pending = Arc::clone(&pending);
        let (wake, receiver) = mpsc::sync_channel(1);
        let (sender, results) = mpsc::channel();
        std::thread::spawn(move || {
            while receiver.recv().is_ok() {
                let image = match worker_pending.lock() {
                    Ok(mut slot) => slot.take(),
                    Err(_) => break,
                };
                if let Some(image) = image {
                    let histogram = HistogramData::compute_from_rgba(&image);
                    if sender.send(HistogramResult { image, histogram }).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            pending,
            wake,
            results,
        }
    }
}

impl HistogramLoader {
    pub fn request(&self, image: Arc<RgbaImage>) {
        if let Ok(mut pending) = self.pending.lock() {
            *pending = Some(image);
        }
        // 有尚未消費的喚醒訊號時，直接由工作執行緒讀取最新影像。
        let _ = self.wake.try_send(());
    }

    pub fn poll(&self) -> Option<HistogramResult> {
        self.results.try_recv().ok()
    }
}
