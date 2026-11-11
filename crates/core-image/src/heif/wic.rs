//! Windows Imaging Component 解碼後端。
//!
//! T0 驗證結論（docs/SPIKE_HEIC_WIC.md）：
//! - WIC 已套用 `irot`，輸出尺寸即為正確方向。
//! - HEIF frame 不回報色彩內容，ICC 由容器探測提供，再以 IWICColorTransform 轉 sRGB。
//! - 每執行緒須自行初始化 COM（MTA）；已是 STA 時沿用，不反初始化。

use std::cell::RefCell;

use image::RgbaImage;
use windows::Win32::Foundation::{GENERIC_READ, RPC_E_CHANGED_MODE};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICBitmapDecoder, IWICBitmapSource,
    IWICImagingFactory, IWICStream, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
    WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{HRESULT, HSTRING, Interface};

use super::{Codec, ColorInfo, HeifInfo, Source};
use crate::error::ImageError;

/// 找不到解碼元件。
const WINCODEC_ERR_COMPONENTNOTFOUND: HRESULT = HRESULT(0x88982F50_u32 as i32);
/// 元件初始化失敗（延伸模組不完整時可能出現）。
const WINCODEC_ERR_COMPONENTINITIALIZEFAILURE: HRESULT = HRESULT(0x88982F8B_u32 as i32);
/// sRGB 的 Exif 色彩空間代碼。
const EXIF_COLOR_SPACE_SRGB: u32 = 1;

/// 每執行緒的 COM 狀態與工廠；欄位依宣告順序釋放，工廠先於 COM 反初始化。
struct WicThread {
    factory: Option<IWICImagingFactory>,
    uninitialize: bool,
}

impl Drop for WicThread {
    fn drop(&mut self) {
        self.factory = None;
        if self.uninitialize {
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    static THREAD: RefCell<WicThread> = RefCell::new(WicThread::init());
}

impl WicThread {
    fn init() -> Self {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        // S_OK／S_FALSE 需配對反初始化；已是 STA（RPC_E_CHANGED_MODE）則沿用既有模型。
        let uninitialize = hr.is_ok();
        debug_assert!(hr.is_ok() || hr == RPC_E_CHANGED_MODE);
        Self {
            factory: None,
            uninitialize,
        }
    }
}

fn factory() -> Result<IWICImagingFactory, ImageError> {
    THREAD.with(|thread| {
        let mut thread = thread.borrow_mut();
        if let Some(factory) = &thread.factory {
            return Ok(factory.clone());
        }
        let factory: IWICImagingFactory =
            unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
                .map_err(|error| platform_error(error, None))?;
        thread.factory = Some(factory.clone());
        Ok(factory)
    })
}

fn platform_error(error: windows::core::Error, codec: Option<Codec>) -> ImageError {
    let code = error.code();
    match codec {
        Some(codec)
            if code == WINCODEC_ERR_COMPONENTNOTFOUND
                || code == WINCODEC_ERR_COMPONENTINITIALIZEFAILURE =>
        {
            ImageError::CodecMissing { codec }
        }
        _ => ImageError::Platform(format!("{} (0x{:08X})", error.message(), code.0 as u32)),
    }
}

/// 解碼器與其記憶體串流；串流不複製緩衝，須與解碼器同生命週期。
struct Opened {
    decoder: IWICBitmapDecoder,
    _stream: Option<IWICStream>,
}

fn open(factory: &IWICImagingFactory, source: Source, codec: Codec) -> Result<Opened, ImageError> {
    let map = |error| platform_error(error, Some(codec));
    unsafe {
        match source {
            Source::Path(path) => Ok(Opened {
                decoder: factory
                    .CreateDecoderFromFilename(
                        &HSTRING::from(path.as_os_str()),
                        None,
                        GENERIC_READ,
                        WICDecodeMetadataCacheOnDemand,
                    )
                    .map_err(map)?,
                _stream: None,
            }),
            Source::Memory(bytes) => {
                let stream = factory.CreateStream().map_err(map)?;
                stream.InitializeFromMemory(bytes).map_err(map)?;
                let decoder = factory
                    .CreateDecoderFromStream(
                        &stream,
                        std::ptr::null(),
                        WICDecodeMetadataCacheOnDemand,
                    )
                    .map_err(map)?;
                Ok(Opened {
                    decoder,
                    _stream: Some(stream),
                })
            }
        }
    }
}

fn first_frame(opened: &Opened, codec: Codec) -> Result<IWICBitmapSource, ImageError> {
    unsafe {
        opened
            .decoder
            .GetFrame(0)
            .and_then(|frame| frame.cast())
            .map_err(|error| platform_error(error, Some(codec)))
    }
}

pub(super) fn dimensions(source: Source, info: &HeifInfo) -> Result<(u32, u32), ImageError> {
    let factory = factory()?;
    let opened = open(&factory, source, info.codec)?;
    let frame = first_frame(&opened, info.codec)?;
    let (mut width, mut height) = (0, 0);
    unsafe { frame.GetSize(&mut width, &mut height) }
        .map_err(|error| platform_error(error, Some(info.codec)))?;
    Ok((width, height))
}

/// 依容器色彩資訊轉 sRGB；ICC 無法使用或轉換失敗時降級回原始來源。
fn color_managed(
    factory: &IWICImagingFactory,
    source: &IWICBitmapSource,
    color: &ColorInfo,
) -> IWICBitmapSource {
    let ColorInfo::Icc(icc) = color else {
        return source.clone();
    };
    let transformed = (|| -> windows::core::Result<IWICBitmapSource> {
        unsafe {
            let from = factory.CreateColorContext()?;
            from.InitializeFromMemory(icc)?;
            let to = factory.CreateColorContext()?;
            to.InitializeFromExifColorSpace(EXIF_COLOR_SPACE_SRGB)?;
            let transform = factory.CreateColorTransformer()?;
            transform.Initialize(source, &from, &to, &GUID_WICPixelFormat32bppRGBA)?;
            transform.cast()
        }
    })();
    transformed.unwrap_or_else(|_| source.clone())
}

fn copy_rgba(
    factory: &IWICImagingFactory,
    source: &IWICBitmapSource,
    codec: Codec,
) -> Result<RgbaImage, ImageError> {
    let map = |error| platform_error(error, Some(codec));
    unsafe {
        let converter = factory.CreateFormatConverter().map_err(map)?;
        converter
            .Initialize(
                source,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(map)?;
        let (mut width, mut height) = (0, 0);
        converter.GetSize(&mut width, &mut height).map_err(map)?;
        let stride = width.checked_mul(4).ok_or(ImageError::ImageTooLarge {
            width,
            height,
            total_pixels: u64::from(width) * u64::from(height),
            max_pixels: u64::from(u32::MAX / 4),
        })?;
        let mut pixels = vec![0u8; stride as usize * height as usize];
        converter
            .CopyPixels(std::ptr::null(), stride, &mut pixels)
            .map_err(map)?;
        RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| ImageError::Platform("解碼輸出尺寸不一致".into()))
    }
}

pub(super) fn decode(source: Source, info: &HeifInfo) -> Result<RgbaImage, ImageError> {
    let factory = factory()?;
    let opened = open(&factory, source, info.codec)?;
    let frame = first_frame(&opened, info.codec)?;
    let managed = color_managed(&factory, &frame, &info.color);
    copy_rgba(&factory, &managed, info.codec)
}

pub(super) fn thumbnail(source: Source, info: &HeifInfo) -> Option<RgbaImage> {
    let factory = factory().ok()?;
    let opened = open(&factory, source, info.codec).ok()?;
    let thumbnail: IWICBitmapSource = unsafe {
        opened
            .decoder
            .GetFrame(0)
            .and_then(|frame| frame.GetThumbnail())
            .ok()?
    };
    let managed = color_managed(&factory, &thumbnail, &info.color);
    copy_rgba(&factory, &managed, info.codec).ok()
}

pub(super) fn icc_usable(icc: &[u8]) -> bool {
    factory().is_ok_and(|factory| unsafe {
        factory
            .CreateColorContext()
            .and_then(|context| context.InitializeFromMemory(icc))
            .is_ok()
    })
}

/// 測試用：以 WIC 解碼任意格式（例如 PNG），驗證 COM、串流與格式轉換管線。
#[cfg(test)]
pub(crate) fn decode_any(
    path: Option<&std::path::Path>,
    bytes: Option<&[u8]>,
) -> Result<RgbaImage, ImageError> {
    let source = match (path, bytes) {
        (Some(path), _) => Source::Path(path),
        (None, Some(bytes)) => Source::Memory(bytes),
        _ => unreachable!(),
    };
    let factory = factory()?;
    let opened = open(&factory, source, Codec::Unknown)?;
    let frame = first_frame(&opened, Codec::Unknown)?;
    copy_rgba(&factory, &frame, Codec::Unknown)
}

/// 測試用：以 WIC HEIF 編碼器產生樣本；編碼元件不存在時回傳 `None`。
#[cfg(test)]
pub(crate) fn encode_heif(pixels: &RgbaImage) -> Option<Vec<u8>> {
    use windows::Win32::Graphics::Imaging::{
        GUID_ContainerFormatHeif, GUID_WICPixelFormat32bppBGRA, WICBitmapEncoderNoCache,
    };
    use windows::Win32::System::Com::StructuredStorage::IPropertyBag2;
    let dir = tempfile::tempdir().ok()?;
    let target = dir.path().join("樣本.heic");
    let factory = factory().ok()?;
    let result = (|| -> windows::core::Result<()> {
        unsafe {
            let stream = factory.CreateStream()?;
            stream.InitializeFromFilename(
                &HSTRING::from(target.as_os_str()),
                windows::Win32::Foundation::GENERIC_WRITE.0,
            )?;
            let encoder = factory.CreateEncoder(&GUID_ContainerFormatHeif, std::ptr::null())?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let mut frame = None;
            let mut options: Option<IPropertyBag2> = None;
            encoder.CreateNewFrame(&mut frame, &mut options)?;
            let frame = frame.ok_or_else(windows::core::Error::empty)?;
            frame.Initialize(options.as_ref())?;
            frame.SetSize(pixels.width(), pixels.height())?;
            let mut format = GUID_WICPixelFormat32bppBGRA;
            frame.SetPixelFormat(&mut format)?;
            // 兩者皆為每像素 4 位元組的 BGR(A) 排列；其他格式不支援產生樣本。
            if format != GUID_WICPixelFormat32bppBGRA
                && format != windows::Win32::Graphics::Imaging::GUID_WICPixelFormat32bppBGR
            {
                return Err(windows::core::Error::empty());
            }
            let mut bgra = pixels.as_raw().clone();
            for px in bgra.as_chunks_mut::<4>().0 {
                px.swap(0, 2);
            }
            frame.WritePixels(pixels.height(), pixels.width() * 4, &bgra)?;
            frame.Commit()?;
            encoder.Commit()?;
            Ok(())
        }
    })();
    result.ok()?;
    std::fs::read(&target).ok()
}
