//! 有界拍攝日期索引：不解碼像素，只交付完整且仍有效的一輪結果。
use std::{
    collections::HashMap,
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::SystemTime,
};

const HEADER_LIMIT: u64 = 512 * 1024;
/// 首段讀取量；多數相機 JPEG 的 Exif APP1 位於此範圍內。
const FIRST_CHUNK: u64 = 64 * 1024;
const CACHE_LIMIT: usize = 100_000;
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp(u64, Option<SystemTime>);
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp(meta.len(), meta.modified().ok()))
}
fn read_date(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = Vec::with_capacity(64);
    (&mut file).take(64).read_to_end(&mut header).ok()?;
    file.seek(SeekFrom::Start(0)).ok()?;
    if core_image::heif::is_heif_bytes(&header) {
        read_heif_date(file)
    } else {
        read_date_from(file)
    }
}

/// 累計讀取量有上限的 reader：超過預算即回傳錯誤；Seek 不計入讀取量。
///
/// 預算同時限制配置量：`kamadak-exif` 依實際讀到的資料增長緩衝，不依宣告長度預先配置
/// （見 docs/SPIKE_HEIC_WIC.md）。
struct BudgetReader<R> {
    inner: R,
    read: u64,
    budget: u64,
}
impl<R: Read> Read for BudgetReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let remaining = self.budget.saturating_sub(self.read);
        if remaining == 0 {
            return Err(std::io::Error::other("超過日期讀取預算"));
        }
        let limit = buf
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        let n = self.inner.read(&mut buf[..limit])?;
        self.read += n as u64;
        Ok(n)
    }
}
impl<R: Seek> Seek for BudgetReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// HEIF 家族：以有界 Seek reader 交給 EXIF 解析，可跳到檔案後段讀取 Exif 區段；
/// 512 KiB 為累計讀取與配置上限，不是檔案位置上限。越界或失敗一律視為無可用日期。
fn read_heif_date<R: Read + Seek>(reader: R) -> Option<String> {
    let budget = BudgetReader {
        inner: reader,
        read: 0,
        budget: HEADER_LIMIT,
    };
    let mut buffered = std::io::BufReader::with_capacity(16 * 1024, budget);
    normalize_date(core_image::exif::read_photo_metadata_from_reader(
        &mut buffered,
    ))
}
/// JPEG 前段是否已足以判定 EXIF；只有確定不需再讀時才回傳 true。
///
/// 逐段走訪標記：讀到影像資料（SOS）或結尾前仍無 Exif APP1，代表沒有 EXIF；
/// Exif APP1 整段都在緩衝區內，代表再讀也不會改變結果。段落被截斷或非 JPEG 一律回傳 false。
fn jpeg_exif_settled(bytes: &[u8]) -> bool {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return false;
    }
    let mut i = 2;
    loop {
        // 標記前可有多個 0xFF 填充位元組。
        while bytes.get(i) == Some(&0xFF) && bytes.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        let (Some(&0xFF), Some(&marker)) = (bytes.get(i), bytes.get(i + 1)) else {
            return false;
        };
        match marker {
            0xDA | 0xD9 => return true,
            0x01 | 0xD0..=0xD7 => {
                i += 2;
                continue;
            }
            _ => {}
        }
        let Some(length) = bytes
            .get(i + 2..i + 4)
            .map(|b| u16::from_be_bytes([b[0], b[1]]) as usize)
        else {
            return false;
        };
        let end = i + 2 + length;
        if marker == 0xE1 && bytes.get(i + 4..i + 10) == Some(b"Exif\0\0") {
            return end <= bytes.len();
        }
        if length < 2 {
            return false;
        }
        i = end;
    }
}
/// 漸進讀取：先讀首段，只有無法確定結果時才補讀至 512 KiB 上限。
fn read_date_from(mut reader: impl Read) -> Option<String> {
    let mut bytes = Vec::new();
    (&mut reader)
        .take(FIRST_CHUNK)
        .read_to_end(&mut bytes)
        .ok()?;
    if let Some(date) = parse_date(&bytes) {
        return Some(date);
    }
    if (bytes.len() as u64) < FIRST_CHUNK || jpeg_exif_settled(&bytes) {
        return None;
    }
    reader
        .take(HEADER_LIMIT - FIRST_CHUNK)
        .read_to_end(&mut bytes)
        .ok()?;
    parse_date(&bytes)
}
fn parse_date(bytes: &[u8]) -> Option<String> {
    normalize_date(core_image::exif::read_photo_metadata_from_reader(
        &mut Cursor::new(bytes),
    ))
}
fn normalize_date(metadata: core_image::exif::PhotoMetadata) -> Option<String> {
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
    /// 組合 JPEG：SOI、指定的前置 APP2 填充段、可選 Exif APP1、SOS 與尾端資料。
    fn jpeg(padding: &[usize], exif: bool, tail: usize) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        for &size in padding {
            out.extend_from_slice(&[0xFF, 0xE2]);
            out.extend_from_slice(&((size + 2) as u16).to_be_bytes());
            out.resize(out.len() + size, 0);
        }
        if exif {
            let mut payload = b"Exif\0\0".to_vec();
            payload.extend(tiff(true, "2024:02:29 12:30:00", 44));
            out.extend_from_slice(&[0xFF, 0xE1]);
            out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            out.extend(payload);
        }
        out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02]);
        out.resize(out.len() + tail, 0x55);
        out
    }
    fn read_counted(bytes: Vec<u8>) -> (Option<String>, u64) {
        let mut cursor = Cursor::new(bytes);
        let date = read_date_from(&mut cursor);
        (date, cursor.position())
    }

    fn boxed(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }
    fn full(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        boxed(kind, &[&[0u8, 0, 0, 0][..], body].concat())
    }
    /// 合成 HEIF：`meta` 在前，Exif 項目位於 `filler` 位元組的 `mdat` 之後；
    /// `declared_len` 可宣告與實際不同的 Exif 長度（模擬損毀或惡意檔案）。
    fn heif_with_exif_at_end(filler: usize, declared_len: Option<u32>) -> Vec<u8> {
        let mut exif = 0u32.to_be_bytes().to_vec();
        exif.extend(tiff(true, "2024:02:29 12:30:00", 44));
        let ftyp = boxed(b"ftyp", b"heic\0\0\0\0mif1heic");
        let mut infe = 1u16.to_be_bytes().to_vec();
        infe.extend_from_slice(&0u16.to_be_bytes());
        infe.extend_from_slice(b"Exif\0");
        let infe = boxed(b"infe", &[&[2u8, 0, 0, 0][..], &infe].concat());
        let iinf = full(b"iinf", &[&1u16.to_be_bytes()[..], &infe].concat());
        // iloc v0：offset_size=4、length_size=4、無 base_offset，1 個項目 1 個 extent。
        let build = |offset: u32| {
            let mut iloc = vec![0x44, 0x00];
            iloc.extend_from_slice(&1u16.to_be_bytes());
            iloc.extend_from_slice(&1u16.to_be_bytes());
            iloc.extend_from_slice(&0u16.to_be_bytes());
            iloc.extend_from_slice(&1u16.to_be_bytes());
            iloc.extend_from_slice(&offset.to_be_bytes());
            iloc.extend_from_slice(&declared_len.unwrap_or(exif.len() as u32).to_be_bytes());
            full(b"meta", &[iinf.clone(), full(b"iloc", &iloc)].concat())
        };
        let meta_len = build(0).len();
        let exif_offset = (ftyp.len() + meta_len + 8 + filler) as u32;
        let mut mdat = vec![0x55; filler];
        mdat.extend(&exif);
        [ftyp, build(exif_offset), boxed(b"mdat", &mdat)].concat()
    }
    fn heif_counted(bytes: Vec<u8>) -> (Option<String>, u64) {
        let mut budget = BudgetReader {
            inner: Cursor::new(bytes),
            read: 0,
            budget: u64::MAX,
        };
        let date = read_heif_date(&mut budget);
        (date, budget.read)
    }

    #[test]
    #[ignore = "需要 temp/ 內的真實 HEIC 樣本"]
    fn 真實_heic_日期索引經有界讀取取得含小數秒的拍攝時間() {
        let sample =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../temp/20250602_112538943_iOS.heic");
        let file = std::fs::File::open(&sample).unwrap();
        let mut budget = BudgetReader {
            inner: file,
            read: 0,
            budget: HEADER_LIMIT,
        };
        assert_eq!(
            read_heif_date(&mut budget).as_deref(),
            Some("2025-06-02 19:25:38.943")
        );
        assert!(budget.read <= 64 * 1024, "實讀 {} bytes", budget.read);
    }

    #[test]
    fn heif_日期以有界_seek_讀取檔尾_exif_且越界降級() {
        let date = Some("2024-02-29 12:30:00".to_owned());
        // Exif 位於 4 MiB 影像資料之後：可 Seek 取得，實際讀取遠小於 512 KiB。
        let file = heif_with_exif_at_end(4 << 20, None);
        assert!(core_image::heif::is_heif_bytes(&file));
        let (found, read) = heif_counted(file.clone());
        assert_eq!(found, date);
        assert!(read < 64 * 1024, "實讀 {read} bytes");
        // 經檔案路徑的完整流程也能讀到。
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("尾端.heic");
        std::fs::write(&path, &file).unwrap();
        assert_eq!(read_date(&path), date);
        // 宣告超大 Exif 長度：達到預算即停止，不得配置或讀取超過上限。
        let huge = heif_with_exif_at_end(1 << 20, Some(u32::MAX));
        let mut reader = BudgetReader {
            inner: Cursor::new(huge),
            read: 0,
            budget: HEADER_LIMIT,
        };
        assert_eq!(read_heif_date(&mut reader), None);
        assert!(reader.read <= HEADER_LIMIT);
        // 截斷檔：不 panic，視為無可用日期。
        for cut in [10, 60, 200, file.len() - 10] {
            assert_eq!(heif_counted(file[..cut].to_vec()).0, None);
        }
    }
    #[test]
    fn 漸進讀取在可確定結果時只讀首段且不漏掉後方_exif() {
        let date = Some("2024-02-29 12:30:00".to_owned());
        // 常見相機 JPEG：Exif 在檔頭，只讀首段。
        assert_eq!(
            read_counted(jpeg(&[], true, 1 << 20)),
            (date.clone(), FIRST_CHUNK)
        );
        // 無 EXIF 的大 JPEG：首段已見影像資料起點，不補讀。
        assert_eq!(
            read_counted(jpeg(&[100], false, 1 << 20)),
            (None, FIRST_CHUNK)
        );
        // 大型 ICC 等前置段把 Exif 推到 64 KiB 之後：補讀後仍取得日期。
        let (found, read) = read_counted(jpeg(&[60_000, 60_000], true, 1 << 20));
        assert_eq!(found, date);
        assert_eq!(read, HEADER_LIMIT);
        // Exif APP1 跨越首段邊界：判定未完整，補讀。
        let (found, _) = read_counted(jpeg(&[FIRST_CHUNK as usize - 100], true, 1 << 20));
        assert_eq!(found, date);
        // 超過上限仍不讀取，維持原有界契約。
        assert_eq!(
            read_counted(jpeg(&[65_000; 9], true, 1 << 20)),
            (None, HEADER_LIMIT)
        );
        // 小檔案一次讀完即結束。
        let small = jpeg(&[], false, 10);
        let length = small.len() as u64;
        assert_eq!(read_counted(small), (None, length));
    }
    #[test]
    fn jpeg_段落判定只在確定時停止() {
        assert!(jpeg_exif_settled(&jpeg(&[], false, 0)));
        assert!(jpeg_exif_settled(&jpeg(&[10], true, 0)));
        let full = jpeg(&[], true, 0);
        assert!(!jpeg_exif_settled(&full[..full.len() - 10]), "APP1 被截斷");
        assert!(
            !jpeg_exif_settled(&[0xFF, 0xD8, 0xFF, 0xE2, 0x00]),
            "段長不完整"
        );
        assert!(
            !jpeg_exif_settled(&[0xFF, 0xD8, 0xFF, 0xE2, 0x00, 0x00]),
            "段長無效"
        );
        assert!(!jpeg_exif_settled(b"II\x2a\0"), "非 JPEG 交由上限讀取");
        assert!(
            jpeg_exif_settled(&[0xFF, 0xD8, 0xFF, 0xFF, 0xD0, 0xFF, 0xD9]),
            "填充與獨立標記"
        );
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
