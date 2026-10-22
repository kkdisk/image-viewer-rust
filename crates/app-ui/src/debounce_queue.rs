//! 像素微調防抖佇列 (Debounce / Latest-Only 非同步運算工作池)

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use core_image::{AdjustmentParams, apply_adjustments_rgba};
use image::RgbaImage;

/// 微調運算請求
struct AdjustmentTask {
    request_id: u64,
    base_image: Arc<RgbaImage>,
    params: AdjustmentParams,
}

/// 微調運算完成結果
pub struct AdjustmentResult {
    pub request_id: u64,
    pub adjusted_image: Arc<RgbaImage>,
}

/// 微調防抖與背景運算排程器
pub struct AdjustmentDebouncer {
    /// 當前最新請求版本編號
    current_request_id: Arc<AtomicU64>,
    /// 發送工作至後台之通道
    sender: Sender<AdjustmentTask>,
    /// 接收運算完成之通道
    receiver: Receiver<AdjustmentResult>,
    /// 是否正在背景計算中
    is_busy: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for AdjustmentDebouncer {
    fn default() -> Self {
        Self::new()
    }
}

impl AdjustmentDebouncer {
    pub fn new() -> Self {
        let (task_sender, task_receiver) = channel::<AdjustmentTask>();
        let (result_sender, result_receiver) = channel::<AdjustmentResult>();

        let current_request_id = Arc::new(AtomicU64::new(0));
        let is_busy = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let worker_req_id = Arc::clone(&current_request_id);
        let worker_busy = Arc::clone(&is_busy);

        // 啟動獨立運算工作執行緒（內部搭配 Rayon 平行加速）
        thread::spawn(move || {
            while let Ok(task) = task_receiver.recv() {
                // Latest-Only 檢查：若已有更新的請求進來，直接丟棄舊請求
                let latest_id = worker_req_id.load(Ordering::Relaxed);
                if task.request_id < latest_id {
                    continue;
                }

                worker_busy.store(true, Ordering::Relaxed);

                // 執行多核平行像素微調運算
                let adjusted = apply_adjustments_rgba(&task.base_image, &task.params);

                // 再次檢查是否在運算期間有新請求進入
                let latest_id_after = worker_req_id.load(Ordering::Relaxed);
                if task.request_id == latest_id_after {
                    let _ = result_sender.send(AdjustmentResult {
                        request_id: task.request_id,
                        adjusted_image: Arc::new(adjusted),
                    });
                }

                worker_busy.store(false, Ordering::Relaxed);
            }
        });

        Self {
            current_request_id,
            sender: task_sender,
            receiver: result_receiver,
            is_busy,
        }
    }

    /// 提交微調參數計算請求（自動遞增版本號，丟棄前序過期運算）
    pub fn request(&self, base_image: Arc<RgbaImage>, params: AdjustmentParams) -> u64 {
        let req_id = self.current_request_id.fetch_add(1, Ordering::SeqCst) + 1;
        let _ = self.sender.send(AdjustmentTask {
            request_id: req_id,
            base_image,
            params,
        });
        req_id
    }

    /// 取得目前最新被接受的請求編號
    pub fn latest_request_id(&self) -> u64 {
        self.current_request_id.load(Ordering::Relaxed)
    }

    /// 輪詢是否有完成的微調計算結果
    pub fn poll(&self) -> Option<AdjustmentResult> {
        let mut latest_result = None;
        // 排空通道只取最新一筆完成結果
        while let Ok(res) = self.receiver.try_recv() {
            if res.request_id == self.latest_request_id() {
                latest_result = Some(res);
            }
        }
        latest_result
    }

    /// 檢查背景運算池是否繁忙
    pub fn is_computing(&self) -> bool {
        self.is_busy.load(Ordering::Relaxed)
    }
}
