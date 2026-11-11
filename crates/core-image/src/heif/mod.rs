//! HEIF 家族（HEIC／AVIF）讀取：容器探測與平台解碼後端。
//!
//! 容器判定、色彩資訊與錯誤對應與平台無關；像素解碼交給平台後端（Windows 為 WIC）。
//! HEIF 的方向以容器 `irot`／`imir` 為準，平台解碼已套用，因此**不再**依 EXIF 校正。

pub mod probe;
#[cfg(windows)]
mod wic;

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use image::{DynamicImage, RgbaImage};

use crate::error::ImageError;
use crate::io::check_pixel_limit;
pub use probe::{Codec, ColorInfo, HeifInfo};

/// 判斷容器家族所需的檔頭長度。
const SNIFF_LEN: usize = 64;

/// 讀取檔頭判斷是否為 HEIF 家族；開檔失敗視為否，交由原流程回報錯誤。
pub fn is_heif_path(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut header = Vec::with_capacity(SNIFF_LEN);
    file.take(SNIFF_LEN as u64).read_to_end(&mut header).is_ok() && probe::is_heif_family(&header)
}

pub fn is_heif_bytes(bytes: &[u8]) -> bool {
    probe::is_heif_family(&bytes[..bytes.len().min(SNIFF_LEN)])
}

fn probe_path(path: &Path) -> Result<HeifInfo, ImageError> {
    let mut reader = BufReader::new(File::open(path)?);
    probe::probe(&mut reader).map_err(|message| ImageError::UnsupportedFormat(message.into()))
}

fn probe_memory(bytes: &[u8]) -> Result<HeifInfo, ImageError> {
    probe::probe_bytes(bytes).map_err(|message| ImageError::UnsupportedFormat(message.into()))
}

/// 是否為 AVIF（依主品牌）；用於顯示名稱。
fn is_avif(info: &HeifInfo) -> bool {
    matches!(&info.major_brand, b"avif" | b"avis") || info.codec == Codec::Av1
}

/// 資訊面板的格式名稱，例如「HEIF（HEVC）」、「AVIF（AV1）」。
pub fn format_label(info: &HeifInfo) -> String {
    let family = if is_avif(info) { "AVIF" } else { "HEIF" };
    format!("{family}（{}）", info.codec.label())
}

fn primaries_name(primaries: u16) -> String {
    match primaries {
        9 => "BT.2020".into(),
        11 => "DCI-P3".into(),
        12 => "Display P3".into(),
        other => format!("色域代碼 {other}"),
    }
}

/// 色彩處理說明；sRGB 或無色彩資訊時回傳 `None`。
pub fn color_note(info: &HeifInfo) -> Option<String> {
    match &info.color {
        ColorInfo::None => None,
        ColorInfo::Nclx { primaries, .. } => (!info.color.is_srgb_like())
            .then(|| format!("色彩未轉換（來源為 {}）", primaries_name(*primaries))),
        ColorInfo::Icc(icc) => Some(if icc_usable(icc) {
            "已依內嵌 ICC 轉換為 sRGB".into()
        } else {
            "色彩未轉換（內嵌 ICC 無法使用）".into()
        }),
    }
}

/// 解碼時的來源：檔案或記憶體。
#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    Path(&'a Path),
    Memory(&'a [u8]),
}

impl Source<'_> {
    fn info(self) -> Result<HeifInfo, ImageError> {
        match self {
            Source::Path(path) => probe_path(path),
            Source::Memory(bytes) => probe_memory(bytes),
        }
    }
}

/// 不支援的編碼：只有 HEVC 與 AV1 會交給平台解碼。
fn ensure_supported(info: &HeifInfo) -> Result<(), ImageError> {
    match info.codec {
        Codec::Hevc | Codec::Av1 => Ok(()),
        other => Err(ImageError::UnsupportedFormat(format!(
            "不支援的 HEIF 編碼：{}",
            other.label()
        ))),
    }
}

pub(crate) fn dimensions(source: Source) -> Result<(u32, u32), ImageError> {
    let info = source.info()?;
    ensure_supported(&info)?;
    platform::dimensions(source, &info)
}

pub(crate) fn decode(source: Source, max_pixels: u64) -> Result<DynamicImage, ImageError> {
    let info = source.info()?;
    ensure_supported(&info)?;
    let (width, height) = platform::dimensions(source, &info)?;
    check_pixel_limit(width, height, max_pixels)?;
    let pixels = platform::decode(source, &info)?;
    Ok(DynamicImage::ImageRgba8(pixels))
}

/// 內嵌縮圖：長邊至少 `min_edge` 才回傳，並套用與主影像相同的色彩處理。
pub fn embedded_thumbnail(path: &Path, min_edge: u32) -> Option<RgbaImage> {
    let info = probe_path(path).ok()?;
    ensure_supported(&info).ok()?;
    let thumbnail = platform::thumbnail(Source::Path(path), &info)?;
    (thumbnail.width().max(thumbnail.height()) >= min_edge).then_some(thumbnail)
}

/// ICC 位元組是否可建立平台色彩內容。
pub fn icc_usable(icc: &[u8]) -> bool {
    platform::icc_usable(icc)
}

#[cfg(windows)]
use wic as platform;

#[cfg(all(test, windows))]
mod tests {
    use super::probe::tests::synth;
    use super::*;
    use crate::io::{inspect_animation, inspect_dimensions, load_image, load_image_with_limit};

    fn gradient(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x * 255 / width) as u8, (y * 255 / height) as u8, 128, 255])
        })
    }

    #[test]
    fn wic_管線以檔案與記憶體解碼_png_並可在背景與_rayon_執行緒使用() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("管線.png");
        let expected = gradient(37, 23);
        expected.save(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(wic::decode_any(Some(&path), None).unwrap(), expected);
        assert_eq!(wic::decode_any(None, Some(&bytes)).unwrap(), expected);
        let thread_path = path.clone();
        let from_thread = std::thread::spawn(move || wic::decode_any(Some(&thread_path), None))
            .join()
            .unwrap()
            .unwrap();
        assert_eq!(from_thread, expected);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        let results: Vec<_> = pool.install(|| {
            use rayon::prelude::*;
            (0..6)
                .into_par_iter()
                .map(|_| wic::decode_any(None, Some(&bytes)).unwrap())
                .collect()
        });
        assert!(results.iter().all(|image| *image == expected));
    }

    #[test]
    fn heif_入口全數分流且不支援或損毀的編碼明確回報() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg_item = dir.path().join("內嵌jpeg.heic");
        std::fs::write(
            &jpeg_item,
            synth(b"mif1", &[], &[(1, b"jpeg")], None, 1, None, 0),
        )
        .unwrap();
        for result in [
            load_image(&jpeg_item).map(|_| ()),
            inspect_dimensions(&jpeg_item).map(|_| ()),
            inspect_animation(&jpeg_item).map(|_| ()),
        ] {
            let message = result.unwrap_err().to_string();
            assert!(message.contains("不支援的 HEIF 編碼"), "{message}");
        }
        // 宣告為 HEVC 但沒有實際影像資料：交給 WIC 後必須回傳錯誤而不是 panic。
        let broken = dir.path().join("損毀.heic");
        std::fs::write(
            &broken,
            synth(b"heic", &[b"mif1"], &[(1, b"hvc1")], None, 1, None, 0),
        )
        .unwrap();
        assert!(load_image(&broken).is_err());
        let bytes = std::fs::read(&broken).unwrap();
        assert!(crate::io::load_image_from_memory(&bytes).is_err());
        assert!(crate::io::inspect_dimensions_from_memory(&bytes).is_err());
    }

    #[test]
    fn 以_wic_產生的_heic_經各入口解碼並遵守像素上限() {
        let source = gradient(64, 48);
        let Some(bytes) = wic::encode_heif(&source) else {
            eprintln!("略過：本機沒有 HEIF／HEVC 編碼元件");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("產生.heic");
        std::fs::write(&path, &bytes).unwrap();
        assert!(is_heif_path(&path) && is_heif_bytes(&bytes));
        let info = probe::probe_bytes(&bytes).unwrap();
        assert_eq!(info.codec, Codec::Hevc);
        assert_eq!(inspect_dimensions(&path).unwrap(), (64, 48));
        assert_eq!(
            crate::io::inspect_dimensions_from_memory(&bytes).unwrap(),
            (64, 48)
        );
        assert!(!inspect_animation(&path).unwrap());
        let decoded = load_image(&path).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (64, 48));
        // 有損編碼：只要求整體接近來源。
        let error: u64 = decoded
            .as_raw()
            .iter()
            .zip(source.as_raw())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        assert!(
            error / decoded.as_raw().len() as u64 <= 8,
            "平均誤差 {}",
            error / decoded.as_raw().len() as u64
        );
        assert_eq!(
            crate::io::load_image_from_memory(&bytes)
                .unwrap()
                .to_rgba8(),
            decoded
        );
        assert!(matches!(
            load_image_with_limit(&path, 64 * 47),
            Err(ImageError::ImageTooLarge { .. })
        ));
        assert_eq!(format_label(&info), "HEIF（HEVC）");
    }

    /// 真實 iPhone 樣本（直式、irot 270°、grid、Display P3）；放在 temp/ 不提交。
    /// 執行：cargo test -p core-image -- --ignored 真實
    #[test]
    #[ignore = "需要 temp/ 內的真實 HEIC 樣本與 HEIF／HEVC 延伸模組"]
    fn 真實_iphone_樣本方向正確且轉換色彩並可取內嵌縮圖() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../temp/20250602_112538943_iOS.heic");
        let info = probe_path(&path).unwrap();
        assert!(matches!(info.color, ColorInfo::Icc(_)));
        assert_eq!(
            color_note(&info).as_deref(),
            Some("已依內嵌 ICC 轉換為 sRGB")
        );
        // ispe 4032×3024 + irot 270°：WIC 已轉正，且不得再依 EXIF（6）旋轉。
        assert_eq!(inspect_dimensions(&path).unwrap(), (3024, 4032));
        let decoded = load_image(&path).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (3024, 4032));
        let plain = wic::decode_any(Some(&path), None).unwrap();
        assert_eq!(plain.dimensions(), decoded.dimensions());
        assert_ne!(plain, decoded, "P3 來源應經色彩轉換");
        let thumbnail = embedded_thumbnail(&path, 256).unwrap();
        assert!(thumbnail.height() > thumbnail.width(), "內嵌縮圖也須是直式");
        assert!(embedded_thumbnail(&path, 1024).is_none());
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{HeifInfo, Source};
    use crate::error::ImageError;
    use image::RgbaImage;

    fn unsupported() -> ImageError {
        ImageError::UnsupportedFormat("此平台尚未支援 HEIF／AVIF 解碼".into())
    }
    pub(super) fn dimensions(_: Source, _: &HeifInfo) -> Result<(u32, u32), ImageError> {
        Err(unsupported())
    }
    pub(super) fn decode(_: Source, _: &HeifInfo) -> Result<RgbaImage, ImageError> {
        Err(unsupported())
    }
    pub(super) fn thumbnail(_: Source, _: &HeifInfo) -> Option<RgbaImage> {
        None
    }
    pub(super) fn icc_usable(_: &[u8]) -> bool {
        false
    }
}
