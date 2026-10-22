//! 有界縮圖工作、背景版本核對與依容量汰換的紋理快取。
use egui::{ColorImage, TextureHandle, TextureOptions};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime},
};
pub const THUMBNAIL_SIZE: u32 = 80;
pub const DEFAULT_THUMBNAIL_BUDGET: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: Option<SystemTime>,
}
fn stamp(path: &Path) -> Result<Stamp, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    Ok(Stamp {
        length: metadata.len(),
        modified: metadata.modified().ok(),
    })
}
struct Request {
    generation: u64,
    path: PathBuf,
    known: Option<Stamp>,
}
struct Completion {
    generation: u64,
    path: PathBuf,
    result: Result<Option<(Stamp, image::RgbaImage)>, String>,
}
struct Entry {
    texture: TextureHandle,
    stamp: Stamp,
    bytes: usize,
    used: u64,
    checked: Instant,
}
pub struct ThumbnailManager {
    textures: HashMap<PathBuf, Entry>,
    pending: HashSet<PathBuf>,
    errors: HashMap<PathBuf, Instant>,
    receiver: mpsc::Receiver<Completion>,
    sender: mpsc::SyncSender<Request>,
    generation: Arc<AtomicU64>,
    scope: Option<PathBuf>,
    budget: usize,
    bytes: usize,
    tick: u64,
}
impl Default for ThumbnailManager {
    fn default() -> Self {
        Self::new()
    }
}
impl ThumbnailManager {
    pub fn new() -> Self {
        Self::with_budget(DEFAULT_THUMBNAIL_BUDGET)
    }
    pub fn with_budget(budget: usize) -> Self {
        let (sender, tasks) = mpsc::sync_channel::<Request>(32);
        let (results, receiver) = mpsc::sync_channel(32);
        let generation = Arc::new(AtomicU64::new(1));
        let tasks = Arc::new(Mutex::new(tasks));
        for _ in 0..2 {
            let tasks = Arc::clone(&tasks);
            let results = results.clone();
            let current = Arc::clone(&generation);
            std::thread::spawn(move || {
                loop {
                    let request = match tasks.lock() {
                        Ok(tasks) => tasks.recv(),
                        Err(_) => break,
                    };
                    let Ok(request) = request else {
                        break;
                    };
                    if request.generation != current.load(Ordering::SeqCst) {
                        continue;
                    }
                    let result = stamp(&request.path).and_then(|before| {
                        if request.known == Some(before) {
                            return Ok(None);
                        }
                        let image = core_image::load_image(&request.path)
                            .map_err(|error| error.to_string())?
                            .thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE)
                            .to_rgba8();
                        if stamp(&request.path)? != before {
                            return Err("解碼期間檔案已更新".into());
                        }
                        Ok(Some((before, image)))
                    });
                    if request.generation == current.load(Ordering::SeqCst)
                        && results
                            .send(Completion {
                                generation: request.generation,
                                path: request.path,
                                result,
                            })
                            .is_err()
                    {
                        break;
                    }
                }
            });
        }
        Self {
            textures: HashMap::new(),
            pending: HashSet::new(),
            errors: HashMap::new(),
            receiver,
            sender,
            generation,
            scope: None,
            budget,
            bytes: 0,
            tick: 0,
        }
    }
    pub fn set_scope(&mut self, scope: Option<PathBuf>) {
        if self.scope != scope {
            self.clear();
            self.scope = scope;
        }
    }
    pub fn cache_bytes(&self) -> usize {
        self.bytes
    }
    pub fn cache_len(&self) -> usize {
        self.textures.len()
    }
    fn remove(&mut self, path: &Path) {
        if let Some(entry) = self.textures.remove(path) {
            self.bytes = self.bytes.saturating_sub(entry.bytes);
        }
    }
    pub fn poll_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.receiver.try_recv() {
            if result.generation != self.generation.load(Ordering::SeqCst) {
                continue;
            }
            self.pending.remove(&result.path);
            match result.result {
                Ok(Some((stamp, image))) => {
                    let bytes = image.as_raw().len();
                    self.remove(&result.path);
                    if bytes > self.budget {
                        continue;
                    }
                    while self.bytes + bytes > self.budget || self.textures.len() >= 512 {
                        let oldest = self
                            .textures
                            .iter()
                            .min_by_key(|(_, entry)| entry.used)
                            .map(|(path, _)| path.clone());
                        if let Some(path) = oldest {
                            self.remove(&path);
                        } else {
                            break;
                        }
                    }
                    let color = ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    );
                    let texture = ctx.load_texture("thumbnail", color, TextureOptions::LINEAR);
                    self.tick += 1;
                    self.bytes += bytes;
                    self.textures.insert(
                        result.path,
                        Entry {
                            texture,
                            stamp,
                            bytes,
                            used: self.tick,
                            checked: Instant::now(),
                        },
                    );
                }
                Ok(None) => {
                    if let Some(entry) = self.textures.get_mut(&result.path) {
                        entry.checked = Instant::now();
                    }
                }
                Err(_) => {
                    self.remove(&result.path);
                    if self.errors.len() >= 64 {
                        self.errors.clear();
                    }
                    self.errors.insert(result.path, Instant::now());
                }
            }
        }
        if !self.pending.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    pub fn get_cached(&self, path: &Path) -> Option<&TextureHandle> {
        self.textures.get(path).map(|entry| &entry.texture)
    }
    pub fn get_or_request(&mut self, path: &Path) -> Option<&TextureHandle> {
        self.tick += 1;
        let mut known = None;
        let mut check = true;
        if let Some(entry) = self.textures.get_mut(path) {
            entry.used = self.tick;
            known = Some(entry.stamp);
            check = entry.checked.elapsed() >= Duration::from_secs(2);
        }
        if check
            && self.pending.len() < 34
            && !self.pending.contains(path)
            && !self
                .errors
                .get(path)
                .is_some_and(|time| time.elapsed() < Duration::from_secs(2))
            && self
                .sender
                .try_send(Request {
                    generation: self.generation.load(Ordering::SeqCst),
                    path: path.to_path_buf(),
                    known,
                })
                .is_ok()
        {
            self.pending.insert(path.to_path_buf());
        }
        self.get_cached(path)
    }
    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.textures.clear();
        self.pending.clear();
        self.errors.clear();
        self.bytes = 0;
        while self.receiver.try_recv().is_ok() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn load(manager: &mut ThumbnailManager, ctx: &egui::Context, path: &Path) {
        let start = Instant::now();
        loop {
            manager.poll_results(ctx);
            manager.get_or_request(path);
            if manager.get_cached(path).is_some() && !manager.pending.contains(path) {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn budget_scope_and_file_updates_are_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut manager = ThumbnailManager::with_budget(80 * 80 * 4);
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        image::RgbaImage::new(80, 80).save(&a).unwrap();
        image::RgbaImage::new(80, 80).save(&b).unwrap();
        manager.set_scope(Some(dir.path().to_path_buf()));
        load(&mut manager, &ctx, &a);
        let texture = manager.get_cached(&a).unwrap().id();
        manager.set_scope(Some(dir.path().to_path_buf()));
        assert_eq!(manager.get_cached(&a).unwrap().id(), texture);
        load(&mut manager, &ctx, &b);
        assert!(manager.cache_bytes() <= 80 * 80 * 4);
        assert_eq!(manager.cache_len(), 1);
        image::RgbaImage::new(40, 80).save(&b).unwrap();
        manager.textures.get_mut(&b).unwrap().checked = Instant::now() - Duration::from_secs(3);
        load(&mut manager, &ctx, &b);
        assert_eq!(manager.get_cached(&b).unwrap().size(), [40, 80]);
        manager.get_or_request(&a);
        manager.clear();
        std::thread::sleep(Duration::from_millis(30));
        manager.poll_results(&ctx);
        assert_eq!(manager.cache_len(), 0);
        assert_eq!(manager.cache_bytes(), 0);
    }
}
