//! 壓縮檔非同步背景解壓縮載入器
//! 將耗時的 ZIP / 7Z 檔案解壓縮與暫存寫入移至背景執行緒，確保 UI 主迴圈絕不卡頓凍結

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use core_archive::{
    ArchiveError, ArchiveLimits, ArchiveProgress, ManagedArchiveSession, ProgressCallback,
};

/// 非同步解壓縮任務
pub struct ArchiveTask {
    pub request_id: u64,
    pub path: PathBuf,
    pub password: Option<String>,
}

/// 非同步解壓縮完成結果
pub struct ArchiveResult {
    pub request_id: u64,
    pub path: PathBuf,
    pub result: Result<ManagedArchiveSession, ArchiveError>,
}

/// 非同步壓縮檔載入排程器
pub struct ArchiveLoader {
    task_sender: Sender<ArchiveTask>,
    result_receiver: Receiver<ArchiveResult>,
    is_busy: Arc<AtomicBool>,
    current_loading_file: Arc<std::sync::Mutex<Option<PathBuf>>>,
    current_progress: Arc<std::sync::Mutex<Option<ArchiveProgress>>>,
    request_counter: Arc<AtomicU64>,
    last_finished_id: Arc<AtomicU64>,
}

impl Default for ArchiveLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl ArchiveLoader {
    pub fn new() -> Self {
        let (task_sender, task_receiver) = channel::<ArchiveTask>();
        let (result_sender, result_receiver) = channel::<ArchiveResult>();

        let is_busy = Arc::new(AtomicBool::new(false));
        let current_loading_file = Arc::new(std::sync::Mutex::new(None));
        let current_progress = Arc::new(std::sync::Mutex::new(None));
        let request_counter = Arc::new(AtomicU64::new(0));
        let last_finished_id = Arc::new(AtomicU64::new(0));

        let worker_busy = Arc::clone(&is_busy);
        let worker_file = Arc::clone(&current_loading_file);
        let worker_progress = Arc::clone(&current_progress);
        let worker_finished = Arc::clone(&last_finished_id);

        // 啟動獨立後台解壓工作執行緒
        thread::Builder::new()
            .name("archive-extractor-worker".to_string())
            .spawn(move || {
                let limits = ArchiveLimits::default();
                while let Ok(task) = task_receiver.recv() {
                    worker_busy.store(true, Ordering::SeqCst);
                    if let Ok(mut lock) = worker_file.lock() {
                        *lock = Some(task.path.clone());
                    }
                    if let Ok(mut lock) = worker_progress.lock() {
                        *lock = None;
                    }

                    let progress_target = Arc::clone(&worker_progress);
                    let progress_cb: ProgressCallback = Arc::new(move |p: ArchiveProgress| {
                        if let Ok(mut lock) = progress_target.lock() {
                            *lock = Some(p);
                        }
                    });

                    let res = ManagedArchiveSession::open_with_progress_and_limits(
                        &task.path,
                        task.password.as_deref(),
                        &limits,
                        Some(progress_cb),
                    );

                    worker_busy.store(false, Ordering::SeqCst);
                    if let Ok(mut lock) = worker_file.lock() {
                        *lock = None;
                    }
                    if let Ok(mut lock) = worker_progress.lock() {
                        *lock = None;
                    }
                    let _ = result_sender.send(ArchiveResult {
                        request_id: task.request_id,
                        path: task.path,
                        result: res,
                    });
                    worker_finished.store(task.request_id, Ordering::SeqCst);
                }
            })
            .expect("無法啟動 archive-extractor-worker 背景執行緒");

        Self {
            task_sender,
            result_receiver,
            is_busy,
            current_loading_file,
            current_progress,
            request_counter,
            last_finished_id,
        }
    }

    /// 發送解壓縮任務至背景執行緒，回傳此任務之專屬流水號 Request ID
    pub fn load_archive<P: AsRef<Path>>(&self, path: P, password: Option<String>) -> u64 {
        let req_id = self.request_counter.fetch_add(1, Ordering::SeqCst) + 1;
        let path_buf = path.as_ref().to_path_buf();
        let _ = self.task_sender.send(ArchiveTask {
            request_id: req_id,
            path: path_buf,
            password,
        });
        req_id
    }

    /// 輪詢是否有已完成之解壓縮結果（非阻塞）
    pub fn poll(&self) -> Option<ArchiveResult> {
        self.result_receiver.try_recv().ok()
    }

    /// 取得最後已完成處理之 Request ID
    pub fn last_finished_id(&self) -> u64 {
        self.last_finished_id.load(Ordering::SeqCst)
    }

    /// 是否正有解壓縮工作在背景執行中
    pub fn is_extracting(&self) -> bool {
        self.is_busy.load(Ordering::Relaxed)
    }

    /// 取得目前正在解壓縮的檔案路徑
    pub fn loading_file_name(&self) -> Option<String> {
        if let Ok(lock) = self.current_loading_file.lock()
            && let Some(path) = &*lock
        {
            return path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }
        None
    }

    /// 取得當前背景解壓之即時進度資訊
    pub fn current_progress(&self) -> Option<ArchiveProgress> {
        if let Ok(lock) = self.current_progress.lock() {
            return lock.clone();
        }
        None
    }
}
