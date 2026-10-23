//! 有界像素工作：一項執行、一項最新等待，調色與幾何共用同一工作執行緒。
use core_image::{AdjustmentParams, apply_adjustments_rgba, geometry::EditOperation};
use image::RgbaImage;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

#[derive(Clone, Copy)]
pub enum PixelOperation {
    Adjust(AdjustmentParams),
    Geometry(EditOperation),
}
struct Task {
    id: u64,
    source: Arc<RgbaImage>,
    identity: Option<(u64, u64)>,
    operation: PixelOperation,
}
pub struct AdjustmentResult {
    pub request_id: u64,
    pub identity: Option<(u64, u64)>,
    pub operation: PixelOperation,
    pub pixels: Result<Arc<RgbaImage>, String>,
}
pub struct AdjustmentDebouncer {
    version: Arc<AtomicU64>,
    pending: Arc<Mutex<Option<Task>>>,
    wake: mpsc::SyncSender<()>,
    result: Arc<Mutex<Option<AdjustmentResult>>>,
    busy: Arc<AtomicBool>,
}
impl Default for AdjustmentDebouncer {
    fn default() -> Self {
        Self::new()
    }
}
impl AdjustmentDebouncer {
    pub fn new() -> Self {
        let version = Arc::new(AtomicU64::new(0));
        let pending = Arc::new(Mutex::new(None::<Task>));
        let busy = Arc::new(AtomicBool::new(false));
        let (wake, requests) = mpsc::sync_channel(1);
        let result = Arc::new(Mutex::new(None));
        let output = result.clone();
        let (v, p, b) = (version.clone(), pending.clone(), busy.clone());
        std::thread::spawn(move || {
            while requests.recv().is_ok() {
                let task = match p.lock() {
                    Ok(mut slot) => {
                        let task = slot.take();
                        // 取走等待工作時即標記，讓取消後切換工具仍能看見尚在執行的工作。
                        if task.is_some() {
                            b.store(true, Ordering::SeqCst);
                        }
                        task
                    }
                    Err(_) => break,
                };
                let Some(task) = task else {
                    continue;
                };
                if task.id != v.load(Ordering::SeqCst) {
                    b.store(false, Ordering::SeqCst);
                    continue;
                }
                let pixels = match task.operation {
                    PixelOperation::Adjust(params) => {
                        Ok(apply_adjustments_rgba(&task.source, &params))
                    }
                    PixelOperation::Geometry(op) => {
                        op.apply(&task.source).map_err(|e| e.to_string())
                    }
                }
                .map(Arc::new);
                b.store(false, Ordering::SeqCst);
                if let Ok(mut slot) = output.lock()
                    && task.id == v.load(Ordering::SeqCst)
                {
                    *slot = Some(AdjustmentResult {
                        request_id: task.id,
                        identity: task.identity,
                        operation: task.operation,
                        pixels,
                    });
                }
            }
        });
        Self {
            version,
            pending,
            wake,
            result,
            busy,
        }
    }
    pub fn request(&self, source: Arc<RgbaImage>, params: AdjustmentParams) -> u64 {
        self.submit(source, None, PixelOperation::Adjust(params))
    }
    pub fn submit(
        &self,
        source: Arc<RgbaImage>,
        identity: Option<(u64, u64)>,
        operation: PixelOperation,
    ) -> u64 {
        // 先排除已完成舊結果，讓最新工作有交付空間。
        if let Ok(mut slot) = self.result.lock() {
            *slot = None;
        }
        let id = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut slot) = self.pending.lock() {
            *slot = Some(Task {
                id,
                source,
                identity,
                operation,
            });
        }
        let _ = self.wake.try_send(());
        id
    }
    pub fn latest_request_id(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }
    pub fn cancel_pending(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut slot) = self.pending.lock() {
            *slot = None;
        }
        if let Ok(mut slot) = self.result.lock() {
            *slot = None;
        }
    }
    pub fn poll(&self) -> Option<AdjustmentResult> {
        let item = self.result.lock().ok()?.take()?;
        (item.request_id == self.latest_request_id()).then_some(item)
    }
    pub fn is_computing(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rapid_requests_keep_bounded_sources_and_deliver_latest() {
        let worker = AdjustmentDebouncer::new();
        let source = Arc::new(RgbaImage::new(64, 64));
        let mut last = 0;
        for _ in 0..1000 {
            last = worker.submit(
                source.clone(),
                Some((7, 3)),
                PixelOperation::Geometry(EditOperation::Resize {
                    width: 32,
                    height: 32,
                    filter: core_image::geometry::ResizeFilter::Pixel,
                }),
            );
        }
        assert!(
            Arc::strong_count(&source) <= 3,
            "來源只應由呼叫端、執行中及最新等待工作持有"
        );
        let start = std::time::Instant::now();
        loop {
            if let Some(result) = worker.poll() {
                assert_eq!(result.request_id, last);
                assert_eq!(result.identity, Some((7, 3)));
                assert_eq!(result.pixels.unwrap().dimensions(), (32, 32));
                break;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        worker.submit(
            source,
            Some((7, 3)),
            PixelOperation::Adjust(AdjustmentParams::default()),
        );
        worker.cancel_pending();
        let start = std::time::Instant::now();
        while worker.is_computing() {
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(worker.poll().is_none());
    }
}
