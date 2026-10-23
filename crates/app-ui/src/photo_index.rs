//! 有界拍攝日期索引：不解碼像素，只交付完整且仍有效的一輪結果。
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::SystemTime,
};

const HEADER_LIMIT: u64 = 512 * 1024;
const CACHE_LIMIT: usize = 100_000;
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp(u64, Option<SystemTime>);
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp(meta.len(), meta.modified().ok()))
}
fn read_date(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(HEADER_LIMIT)
        .read_to_end(&mut bytes)
        .ok()?;
    let metadata = core_image::exif::read_photo_metadata_from_reader(&mut Cursor::new(bytes));
    let mut date = (metadata.datetime_source == Some("拍攝時間"))
        .then_some(metadata.captured_at)
        .flatten()?;
    // 等值小數秒採相同索引鍵，避免 .10 與 .1 被誤認為不同時刻。
    if date.contains('.') {
        date.truncate(date.trim_end_matches('0').trim_end_matches('.').len());
    }
    Some(date)
}
struct Job {
    id: u64,
    revision: u64,
    scope: Option<PathBuf>,
    paths: Vec<PathBuf>,
}
pub(crate) struct Completion {
    pub id: u64,
    pub revision: u64,
    pub dates: HashMap<PathBuf, String>,
    pub days: Vec<String>,
}
pub(crate) struct PhotoIndex {
    version: Arc<AtomicU64>,
    pending: Arc<Mutex<Option<Job>>>,
    wake: mpsc::SyncSender<()>,
    output: Arc<Mutex<Option<Completion>>>,
    progress: Arc<AtomicUsize>,
}
impl Default for PhotoIndex {
    fn default() -> Self {
        let version = Arc::new(AtomicU64::new(0));
        let pending = Arc::new(Mutex::new(None::<Job>));
        let output = Arc::new(Mutex::new(None));
        let progress = Arc::new(AtomicUsize::new(0));
        let (wake, receiver) = mpsc::sync_channel(1);
        let (v, p, o, count) = (
            version.clone(),
            pending.clone(),
            output.clone(),
            progress.clone(),
        );
        std::thread::spawn(move || {
            let mut scope = None;
            let mut cache = HashMap::<PathBuf, (Stamp, Option<String>)>::new();
            while receiver.recv().is_ok() {
                let Some(job) = p.lock().ok().and_then(|mut slot| slot.take()) else {
                    continue;
                };
                if job.id != v.load(Ordering::SeqCst) {
                    continue;
                }
                if scope != job.scope {
                    cache.clear();
                    scope = job.scope;
                }
                let mut dates = HashMap::new();
                for (i, path) in job.paths.iter().enumerate() {
                    if job.id != v.load(Ordering::SeqCst) {
                        break;
                    }
                    if let Some(before) = stamp(path) {
                        let date = match cache.get(path) {
                            Some((known, date)) if *known == before => date.clone(),
                            _ => read_date(path),
                        };
                        if stamp(path) == Some(before) {
                            if cache.len() < CACHE_LIMIT || cache.contains_key(path) {
                                cache.insert(path.clone(), (before, date.clone()));
                            }
                            if let Some(date) = date {
                                dates.insert(path.clone(), date);
                            }
                        } else {
                            cache.remove(path);
                        }
                    } else {
                        cache.remove(path);
                    }
                    if job.id == v.load(Ordering::SeqCst) {
                        count.store(i + 1, Ordering::SeqCst);
                    }
                }
                let mut days: Vec<_> = dates.values().map(|date| date[..10].to_owned()).collect();
                days.sort_unstable();
                days.dedup();
                if let Ok(mut slot) = o.lock()
                    && job.id == v.load(Ordering::SeqCst)
                {
                    *slot = Some(Completion {
                        id: job.id,
                        revision: job.revision,
                        dates,
                        days,
                    });
                }
            }
        });
        Self {
            version,
            pending,
            wake,
            output,
            progress,
        }
    }
}
impl PhotoIndex {
    pub fn request(&self, revision: u64, scope: Option<PathBuf>, paths: Vec<PathBuf>) -> u64 {
        let id = self.version.fetch_add(1, Ordering::SeqCst) + 1;
        self.progress.store(0, Ordering::SeqCst);
        *self.output.lock().unwrap() = None;
        *self.pending.lock().unwrap() = Some(Job {
            id,
            revision,
            scope,
            paths,
        });
        let _ = self.wake.try_send(());
        id
    }
    pub fn cancel(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
        *self.pending.lock().unwrap() = None;
        *self.output.lock().unwrap() = None;
    }
    pub fn progress(&self) -> usize {
        self.progress.load(Ordering::SeqCst)
    }
    pub fn poll(&self) -> Option<Completion> {
        self.output
            .lock()
            .ok()?
            .take()
            .filter(|r| r.id == self.version.load(Ordering::SeqCst))
    }
}
impl Drop for PhotoIndex {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // 僅建構 TIFF 元資料，不需要任何像素或額外測試依賴。
    fn tiff(original: bool, date: &str, offset: u32) -> Vec<u8> {
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        let entry = |out: &mut Vec<u8>, tag: u16, kind: u16, count: u32, value: u32| {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        };
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        if original {
            entry(&mut bytes, 0x8769, 4, 1, 26);
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&1_u16.to_le_bytes());
        }
        entry(
            &mut bytes,
            if original { 0x9003 } else { 0x0132 },
            2,
            20,
            offset,
        );
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.resize(offset as usize, 0);
        bytes.extend_from_slice(date.as_bytes());
        bytes.push(0);
        bytes
    }
    fn wait(index: &PhotoIndex) -> Completion {
        let start = std::time::Instant::now();
        loop {
            if let Some(result) = index.poll() {
                return result;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn 日期只採有效拍攝時間且限制檔頭讀取() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("日期.tiff");
        let good = tiff(true, "2024:02:29 12:30:00", 44);
        std::fs::write(&path, &good).unwrap();
        assert_eq!(read_date(&path).as_deref(), Some("2024-02-29 12:30:00"));
        assert_eq!(std::fs::read(&path).unwrap(), good);
        for bytes in [
            tiff(false, "2024:02:29 12:30:00", 26),
            tiff(true, "2023:02:29 12:30:00", 44),
            tiff(true, "2024:02:29 12:30:00", HEADER_LIMIT as u32 + 10),
            "損壞資料".as_bytes().to_vec(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(read_date(&path).is_none());
        }
    }
    #[test]
    fn 取消過期工作不交付且最新工作及快取失效仍可完成() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("日期.tiff");
        std::fs::write(&path, tiff(true, "2024:02:29 12:30:00", 44)).unwrap();
        let index = PhotoIndex::default();
        let scope = Some(dir.path().to_path_buf());
        for _ in 0..100 {
            index.request(1, scope.clone(), vec![path.clone(); 100]);
        }
        index.cancel();
        assert!(index.poll().is_none());
        let id = index.request(2, scope.clone(), vec![path.clone()]);
        let result = wait(&index);
        assert_eq!((result.id, result.revision), (id, 2));
        assert_eq!(result.dates[&path], "2024-02-29 12:30:00");
        // 大小變更即可使快取失效，不依賴檔案系統時間解析度。
        std::fs::write(&path, tiff(true, "2025:01:01 09:00:00", 80)).unwrap();
        index.request(3, scope, vec![path.clone()]);
        assert_eq!(wait(&index).dates[&path], "2025-01-01 09:00:00");
        std::fs::remove_file(&path).unwrap();
        index.request(4, Some(dir.path().join("新目錄")), vec![path]);
        assert!(wait(&index).dates.is_empty());
    }
}
