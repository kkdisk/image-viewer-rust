//! 非同步縮圖生成佇列與 GPU 紋理快取管理

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use egui::{ColorImage, TextureHandle, TextureOptions};
use image::{imageops::FilterType, RgbaImage};

/// 縮圖預設高寬像素大小
pub const THUMBNAIL_SIZE: u32 = 80;

/// 後台縮圖解碼結果
struct ThumbnailResult {
    path: PathBuf,
    image: RgbaImage,
}

/// 縮圖管理器
pub struct ThumbnailManager {
    /// 已生成並上傳至 GPU 的縮圖快取
    textures: HashMap<PathBuf, TextureHandle>,
    /// 正在後台佇列中等待或執行載入的路徑集合
    pending_paths: Arc<Mutex<HashSet<PathBuf>>>,
    /// 接收後台完成之結果通道
    receiver: Receiver<ThumbnailResult>,
    /// 傳送工作給後台之通道
    task_sender: Sender<PathBuf>,
}

impl Default for ThumbnailManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThumbnailManager {
    pub fn new() -> Self {
        let (result_sender, receiver) = channel::<ThumbnailResult>();
        let (task_sender, task_receiver) = channel::<PathBuf>();
        let pending_paths = Arc::new(Mutex::new(HashSet::new()));

        let worker_pending = Arc::clone(&pending_paths);

        // 啟動專屬背景背景工作執行緒池（2 個工作執行緒）
        let task_receiver = Arc::new(Mutex::new(task_receiver));
        for _ in 0..2 {
            let rx = Arc::clone(&task_receiver);
            let tx = result_sender.clone();
            let pending = Arc::clone(&worker_pending);

            thread::spawn(move || {
                while let Ok(path) = {
                    let lock = rx.lock().unwrap();
                    lock.recv()
                } {
                    // 自磁碟解碼並縮圖
                    if let Ok(dyn_img) = core_image::load_image(&path) {
                        let thumb = dyn_img.resize_to_fill(
                            THUMBNAIL_SIZE,
                            THUMBNAIL_SIZE,
                            FilterType::Triangle,
                        );
                        let rgba = thumb.to_rgba8();
                        let _ = tx.send(ThumbnailResult {
                            path: path.clone(),
                            image: rgba,
                        });
                    }

                    // 自 pending 集合移除
                    if let Ok(mut set) = pending.lock() {
                        set.remove(&path);
                    }
                }
            });
        }

        Self {
            textures: HashMap::new(),
            pending_paths,
            receiver,
            task_sender,
        }
    }

    /// 輪詢並接收後台已完成的縮圖，上傳至 egui GPU 紋理
    pub fn poll_results(&mut self, ctx: &egui::Context) {
        while let Ok(res) = self.receiver.try_recv() {
            let size = [res.image.width() as usize, res.image.height() as usize];
            let color_img = ColorImage::from_rgba_unmultiplied(size, res.image.as_raw());
            let handle = ctx.load_texture(
                format!("thumb_{}", res.path.to_string_lossy()),
                color_img,
                TextureOptions::LINEAR,
            );
            self.textures.insert(res.path, handle);
        }
    }

    /// 取得指定路徑的 GPU 縮圖紋理；若尚未載入則向後台提交請求
    pub fn get_or_request(&mut self, path: &Path) -> Option<&TextureHandle> {
        if self.textures.contains_key(path) {
            return self.textures.get(path);
        }

        // 若尚未在 pending 佇列中，發送至背景載入
        let should_request = if let Ok(mut set) = self.pending_paths.lock() {
            if set.len() < 300 && !set.contains(path) {
                set.insert(path.to_path_buf());
                true
            } else {
                false
            }
        } else {
            false
        };

        if should_request {
            let _ = self.task_sender.send(path.to_path_buf());
        }

        None
    }

    /// 清空所有快取（切換目錄時使用）
    pub fn clear(&mut self) {
        self.textures.clear();
        if let Ok(mut set) = self.pending_paths.lock() {
            set.clear();
        }
    }
}
