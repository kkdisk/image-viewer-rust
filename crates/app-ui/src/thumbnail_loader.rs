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
/// 膠卷列縮圖長邊；卡片小，維持低記憶體成本。
pub const THUMBNAIL_SIZE: u32 = 80;
pub const DEFAULT_THUMBNAIL_BUDGET: usize = 16 * 1024 * 1024;
/// 總覽縮圖長邊層級；以少數層級避免視窗拖曳時頻繁重建。
const GRID_EDGE_TIERS: [u32; 5] = [256, 384, 512, 768, 1024];
/// 依實際圖片區選擇長邊層級。
///
/// 任一長寬比的來源等比例放入 W×H 圖片區時，顯示長邊不超過 max(W, H)；
/// 因此長邊 ≥ max(W, H) × DPI 即可保證方形、橫幅、全景與直幅都不放大（上限 1024）。
/// 需要更大層級時立即升級；只有需求降到目前一半以下才降級，避免來回清除快取。
pub fn grid_thumbnail_edge(current: u32, image_box: egui::Vec2, pixels_per_point: f32) -> u32 {
    let needed = (image_box.x.max(image_box.y) * pixels_per_point).ceil() as u32;
    let tier = GRID_EDGE_TIERS
        .into_iter()
        .find(|tier| *tier >= needed)
        .unwrap_or(GRID_EDGE_TIERS[GRID_EDGE_TIERS.len() - 1]);
    if tier > current || tier * 2 <= current {
        tier
    } else {
        current
    }
}
/// 總覽紋理預算下限；一般 4K 一倍 DPI 整頁（約 288 格 × 175 KiB）即可容納。
pub const GRID_THUMBNAIL_BUDGET: usize = 64 * 1024 * 1024;
/// 總覽紋理預算上限；極端 DPI 或超大螢幕時仍維持有界。
pub const GRID_THUMBNAIL_BUDGET_MAX: usize = 256 * 1024 * 1024;
/// 可見格數 × 最壞（方形）縮圖位元組 × 1.5 倍餘裕，避免整頁縮圖互相汰換而反覆解碼。
pub fn grid_thumbnail_budget(visible_cells: usize, edge: u32) -> usize {
    let worst = (edge as usize).pow(2) * 4;
    (visible_cells.saturating_mul(worst) / 2 * 3)
        .clamp(GRID_THUMBNAIL_BUDGET, GRID_THUMBNAIL_BUDGET_MAX)
}
/// 只縮小不放大：小圖保留原尺寸，避免浪費快取容量。
fn make_thumbnail(image: image::DynamicImage, edge: u32) -> image::RgbaImage {
    if image.width() <= edge && image.height() <= edge {
        image.to_rgba8()
    } else {
        image.thumbnail(edge, edge).to_rgba8()
    }
}
/// 縮圖的像素來源，供測試確認是否走快速路徑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThumbnailSource {
    /// HEIF 檔內嵌縮圖（約 30 ms，見 docs/SPIKE_HEIC_WIC.md）
    Embedded,
    /// 完整解碼後縮小
    Decoded,
}
/// 產生縮圖：HEIF 內嵌縮圖長邊足夠時直接使用（已套用方向與色彩轉換），否則完整解碼。
fn decode_thumbnail(path: &Path, edge: u32) -> Result<(image::RgbaImage, ThumbnailSource), String> {
    if core_image::heif::is_heif_path(path)
        && let Some(embedded) = core_image::heif::embedded_thumbnail(path, edge)
    {
        let image = make_thumbnail(image::DynamicImage::ImageRgba8(embedded), edge);
        return Ok((image, ThumbnailSource::Embedded));
    }
    let image = core_image::load_image(path).map_err(|error| error.to_string())?;
    Ok((make_thumbnail(image, edge), ThumbnailSource::Decoded))
}
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
    edge: u32,
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
    edge: u32,
    bytes: usize,
    tick: u64,
}
impl Default for ThumbnailManager {
    fn default() -> Self {
        Self::new()
    }
}
impl ThumbnailManager {
    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }
    pub fn new() -> Self {
        Self::with_budget(DEFAULT_THUMBNAIL_BUDGET)
    }
    /// 縮圖總覽專用：與膠卷列分開快取，不互相擠出，也不放大膠卷列成本。
    pub fn for_grid() -> Self {
        Self::with_budget(GRID_THUMBNAIL_BUDGET).with_edge(GRID_EDGE_TIERS[0])
    }
    /// 調整容量上限；降低時立即依最近使用汰換，不影響進行中工作。
    pub fn set_budget(&mut self, budget: usize) {
        if self.budget != budget {
            self.budget = budget;
            self.evict_for(0);
        }
    }
    /// 不再顯示時放棄已排工作，避免兩組管理器同時解碼；保留已完成的紋理。
    pub fn cancel_pending(&mut self) {
        if !self.pending.is_empty() {
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.pending.clear();
            while self.receiver.try_recv().is_ok() {}
        }
    }
    fn evict_for(&mut self, incoming: usize) {
        while self.bytes + incoming > self.budget || self.textures.len() >= 512 {
            let oldest = self
                .textures
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(path, _)| path.clone());
            match oldest {
                Some(path) => self.remove(&path),
                None => break,
            }
        }
    }
    fn with_edge(mut self, edge: u32) -> Self {
        self.edge = edge;
        self
    }
    /// 尺寸是快取識別的一部分；變更層級時清除舊尺寸紋理並使進行中工作失效。
    pub fn set_edge(&mut self, edge: u32) {
        if self.edge != edge {
            self.clear();
            self.edge = edge;
        }
    }
    pub fn edge(&self) -> u32 {
        self.edge
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
                        let (image, _) = decode_thumbnail(&request.path, request.edge)?;
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
            edge: THUMBNAIL_SIZE,
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
                    self.evict_for(bytes);
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
                    edge: self.edge,
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

impl crate::app::ImageViewerApp {
    /// 來源清單重建或刪除後，兩組縮圖快取需一併失效。
    pub(crate) fn clear_thumbnails(&mut self) {
        self.thumbnail_manager.clear();
        self.grid_thumbnails.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 非_heif_縮圖維持完整解碼() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("一般.png");
        image::RgbaImage::new(300, 200).save(&path).unwrap();
        let (image, source) = decode_thumbnail(&path, THUMBNAIL_SIZE).unwrap();
        assert_eq!(source, ThumbnailSource::Decoded);
        assert_eq!(image.width(), THUMBNAIL_SIZE);
    }

    #[test]
    #[ignore = "需要 temp/ 內的真實 HEIC 樣本與 HEIF／HEVC 延伸模組"]
    fn 真實_heic_縮圖列與_256_層級用內嵌縮圖_較大層級完整解碼() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../temp/20250602_112538943_iOS.heic");
        for edge in [THUMBNAIL_SIZE, 256] {
            let start = Instant::now();
            let (image, source) = decode_thumbnail(&path, edge).unwrap();
            assert_eq!(source, ThumbnailSource::Embedded, "長邊 {edge}");
            assert_eq!(image.height(), edge, "直式：長邊為高");
            assert!(image.width() < image.height());
            eprintln!("內嵌縮圖 {edge}：{:?}", start.elapsed());
        }
        let (image, source) = decode_thumbnail(&path, 384).unwrap();
        assert_eq!(
            source,
            ThumbnailSource::Decoded,
            "內嵌縮圖長邊 320 不足 384"
        );
        assert_eq!(image.height(), 384);
    }
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
    fn 縮圖只縮小不放大且層級依圖片區長邊並具遲滯() {
        let small = make_thumbnail(image::DynamicImage::new_rgba8(40, 80), 256);
        assert_eq!(small.dimensions(), (40, 80));
        let tier = |w: f32, h: f32, ppp: f32| grid_thumbnail_edge(0, egui::vec2(w, h), ppp);
        assert_eq!(tier(138.0, 116.0, 1.0), 256);
        assert_eq!(tier(138.0, 116.0, 2.0), 384);
        // 審查情境：一欄 276×116 點、200% DPI 需 552px，須升至 768 層級。
        assert_eq!(tier(276.0, 116.0, 2.0), 768);
        assert_eq!(tier(2000.0, 116.0, 3.0), 1024, "超出層級時維持有界上限");
        // 升級立即生效；小幅縮回不降級，降至一半以下才降級。
        assert_eq!(grid_thumbnail_edge(384, egui::vec2(276.0, 116.0), 2.0), 768);
        assert_eq!(grid_thumbnail_edge(768, egui::vec2(250.0, 116.0), 2.0), 768);
        assert_eq!(grid_thumbnail_edge(768, egui::vec2(138.0, 116.0), 2.0), 384);
        assert_eq!(grid_thumbnail_edge(768, egui::vec2(138.0, 116.0), 1.0), 256);
        assert_eq!(grid_thumbnail_budget(1, 256), GRID_THUMBNAIL_BUDGET);
        assert_eq!(
            grid_thumbnail_budget(10_000, 1024),
            GRID_THUMBNAIL_BUDGET_MAX
        );
        assert!(grid_thumbnail_budget(72, 768) >= 72 * 768 * 768 * 4);
    }
    #[test]
    fn 取消排隊工作保留已完成紋理且降低預算立即汰換() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        image::RgbaImage::new(80, 80).save(&a).unwrap();
        image::RgbaImage::new(80, 80).save(&b).unwrap();
        let mut manager = ThumbnailManager::new();
        load(&mut manager, &ctx, &a);
        manager.get_or_request(&b);
        assert_eq!(manager.pending_count(), 1);
        manager.cancel_pending();
        assert_eq!(manager.pending_count(), 0);
        std::thread::sleep(Duration::from_millis(50));
        manager.poll_results(&ctx);
        assert!(manager.get_cached(&b).is_none(), "取消後的舊結果不得交付");
        assert!(manager.get_cached(&a).is_some());
        load(&mut manager, &ctx, &b);
        assert_eq!(manager.cache_len(), 2);
        manager.set_budget(80 * 80 * 4);
        assert_eq!(manager.cache_len(), 1);
        assert!(manager.get_cached(&b).is_some(), "保留最近使用的紋理");
    }
    #[test]
    fn 變更總覽尺寸層級清除舊紋理並以新尺寸重建() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let path = dir.path().join("大圖.png");
        image::RgbaImage::new(600, 400).save(&path).unwrap();
        let mut manager = ThumbnailManager::for_grid();
        assert_eq!(manager.edge(), 256);
        load(&mut manager, &ctx, &path);
        assert_eq!(manager.get_cached(&path).unwrap().size(), [256, 171]);
        manager.set_edge(256);
        assert_eq!(manager.cache_len(), 1, "相同層級不得清除快取");
        manager.set_edge(384);
        assert_eq!(manager.cache_len(), 0);
        load(&mut manager, &ctx, &path);
        assert_eq!(manager.get_cached(&path).unwrap().size(), [384, 256]);
        assert_eq!(ThumbnailManager::new().edge(), THUMBNAIL_SIZE);
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
