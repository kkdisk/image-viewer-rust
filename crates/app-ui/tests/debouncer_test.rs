//! 微調防抖與 Latest-Only 排程測試

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

#[test]
fn test_latest_only_semantics() {
    let current_id = Arc::new(AtomicU64::new(0));
    let (tx, rx) = channel::<u64>();
    let (res_tx, res_rx) = channel::<u64>();

    let worker_id = Arc::clone(&current_id);

    // 模擬背景工作執行緒
    thread::spawn(move || {
        while let Ok(task_id) = rx.recv() {
            // 模擬運算耗時
            thread::sleep(Duration::from_millis(20));

            // Latest-Only 檢查：若已有更新的任務則捨棄
            if task_id == worker_id.load(Ordering::Relaxed) {
                let _ = res_tx.send(task_id);
            }
        }
    });

    // 連續快速發送 5 個請求
    for i in 1..=5 {
        current_id.store(i, Ordering::Relaxed);
        let _ = tx.send(i);
    }

    // 等待背景完成
    thread::sleep(Duration::from_millis(150));

    // 驗證只接收到最新一個結果
    let mut results = Vec::new();
    while let Ok(res) = res_rx.try_recv() {
        results.push(res);
    }

    assert_eq!(results, vec![5]);
}
