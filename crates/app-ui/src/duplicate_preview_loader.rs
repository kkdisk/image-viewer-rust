//! 有界的查重預覽背景工作；關閉或重新掃描時丟棄舊世代。

use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

pub struct PreviewResult {
    pub generation: u64,
    pub path: PathBuf,
    pub pixels: Result<image::RgbaImage, String>,
}

pub struct DuplicatePreviewLoader {
    requests: mpsc::SyncSender<(u64, PathBuf)>,
    results: mpsc::Receiver<PreviewResult>,
    generation: Arc<AtomicU64>,
    pending: HashSet<PathBuf>,
}

impl Default for DuplicatePreviewLoader {
    fn default() -> Self {
        let (requests, tasks) = mpsc::sync_channel::<(u64, PathBuf)>(2);
        let (sender, results) = mpsc::sync_channel(2);
        let generation = Arc::new(AtomicU64::new(1));
        let current = Arc::clone(&generation);
        std::thread::spawn(move || {
            while let Ok((generation, path)) = tasks.recv() {
                if current.load(Ordering::SeqCst) != generation {
                    continue;
                }
                let pixels = core_image::load_image(&path)
                    .map(|image| image.thumbnail(256, 256).to_rgba8())
                    .map_err(|error| error.to_string());
                if current.load(Ordering::SeqCst) == generation
                    && sender
                        .send(PreviewResult {
                            generation,
                            path,
                            pixels,
                        })
                        .is_err()
                {
                    break;
                }
            }
        });
        Self {
            requests,
            results,
            generation,
            pending: HashSet::new(),
        }
    }
}

impl DuplicatePreviewLoader {
    pub fn request(&mut self, path: PathBuf) {
        if self.pending.contains(&path) || self.pending.len() >= 4 {
            return;
        }
        if self
            .requests
            .try_send((self.generation.load(Ordering::SeqCst), path.clone()))
            .is_ok()
        {
            self.pending.insert(path);
        }
    }

    pub fn poll(&mut self) -> Option<PreviewResult> {
        while let Ok(result) = self.results.try_recv() {
            if result.generation == self.generation.load(Ordering::SeqCst) {
                self.pending.remove(&result.path);
                return Some(result);
            }
        }
        None
    }

    pub fn reset(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.pending.clear();
        while self.results.try_recv().is_ok() {}
    }

    pub fn is_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(loader: &mut DuplicatePreviewLoader) -> PreviewResult {
        let start = Instant::now();
        loop {
            if let Some(result) = loader.poll() {
                return result;
            }
            assert!(start.elapsed() < Duration::from_secs(5), "背景預覽逾時");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn preview_decodes_safely_in_background_and_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preview.png");
        image::RgbaImage::new(512, 128).save(&path).unwrap();
        let mut loader = DuplicatePreviewLoader::default();
        loader.request(path.clone());
        let result = wait(&mut loader);
        assert_eq!(result.path, path);
        assert_eq!(result.pixels.unwrap().dimensions(), (256, 64));
        std::fs::write(&path, "損毀圖片".as_bytes()).unwrap();
        loader.request(path);
        assert!(wait(&mut loader).pixels.is_err());
        assert!(!loader.is_pending());
    }

    #[test]
    fn reset_discards_old_results_and_accepts_new_generation() {
        let mut loader = DuplicatePreviewLoader::default();
        loader.request("舊的不存在.png".into());
        loader.reset();
        let current = loader.generation.load(Ordering::SeqCst);
        loader.request("新的不存在.png".into());
        let result = wait(&mut loader);
        assert_eq!(result.generation, current);
        assert_eq!(result.path, PathBuf::from("新的不存在.png"));
        assert!(result.pixels.is_err());
    }

    #[test]
    fn oversized_header_is_rejected_before_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oversized.bmp");
        image::RgbaImage::new(1, 1).save(&path).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[18..22].copy_from_slice(&100_000_i32.to_le_bytes());
        bytes[22..26].copy_from_slice(&100_000_i32.to_le_bytes());
        std::fs::write(&path, bytes).unwrap();
        let mut loader = DuplicatePreviewLoader::default();
        loader.request(path);
        assert!(wait(&mut loader).pixels.is_err());
    }
}
