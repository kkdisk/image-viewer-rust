//! 影像安全載入與原子儲存模組

use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor, Seek, SeekFrom, Write};
use std::path::Path;

use image::{DynamicImage, ImageFormat, ImageReader};
use tempfile::Builder;

use crate::error::ImageError;
use crate::exif::{apply_orientation, read_orientation};

/// 安全檢查尺寸後，最多讀取兩格以辨識 GIF／WebP 動畫；不遍歷完整動畫。
pub fn inspect_animation<P: AsRef<Path>>(path: P) -> Result<bool, ImageError> {
    use image::AnimationDecoder;
    let reader = ImageReader::open(path.as_ref())?.with_guessed_format()?;
    let format = reader.format();
    let (width, height) = reader.into_dimensions()?;
    check_pixel_limit(width, height, MAX_SAFE_PIXELS)?;
    let file = BufReader::new(File::open(path)?);
    let mut frames = match format {
        Some(ImageFormat::Gif) => image::codecs::gif::GifDecoder::new(file)?.into_frames(),
        Some(ImageFormat::WebP) => image::codecs::webp::WebPDecoder::new(file)?.into_frames(),
        _ => return Ok(false),
    };
    if let Some(frame) = frames.next() {
        frame?;
    } else {
        return Ok(false);
    }
    match frames.next() {
        Some(frame) => {
            frame?;
            Ok(true)
        }
        None => Ok(false),
    }
}

/// 預設最大安全像素上限：64M 像素 (64 * 1024 * 1024 = 67,108,864 像素)
pub const MAX_SAFE_PIXELS: u64 = 64 * 1024 * 1024;

/// 檢查像素是否超出安全限制
#[inline]
pub fn check_pixel_limit(width: u32, height: u32, max_pixels: u64) -> Result<(), ImageError> {
    let total_pixels = (width as u64) * (height as u64);
    if total_pixels > max_pixels {
        return Err(ImageError::ImageTooLarge {
            width,
            height,
            total_pixels,
            max_pixels,
        });
    }
    Ok(())
}

/// 僅讀取影像標頭以獲取圖片尺寸 (width, height)，不解碼像素資料
pub fn inspect_dimensions<P: AsRef<Path>>(path: P) -> Result<(u32, u32), ImageError> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    Ok((width, height))
}

/// 自記憶體二進位資料讀取影像標頭以獲取圖片尺寸 (width, height)
pub fn inspect_dimensions_from_memory(bytes: &[u8]) -> Result<(u32, u32), ImageError> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    Ok((width, height))
}

/// 載入影像並依據 EXIF 自動校正方向，預設安全上限為 64M 像素
pub fn load_image<P: AsRef<Path>>(path: P) -> Result<DynamicImage, ImageError> {
    load_image_with_limit(path, MAX_SAFE_PIXELS)
}

/// 載入指定路徑影像，具備自訂最大像素保護上限與 EXIF Orientation 自動校正
pub fn load_image_with_limit<P: AsRef<Path>>(
    path: P,
    max_pixels: u64,
) -> Result<DynamicImage, ImageError> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref)?;
    let mut buf_reader = BufReader::new(file);

    // 1. 預檢尺寸以防範解壓炸彈
    let reader = ImageReader::new(&mut buf_reader).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    check_pixel_limit(width, height, max_pixels)?;

    // 2. 嘗試讀取 EXIF Orientation
    buf_reader.seek(SeekFrom::Start(0))?;
    let orientation = read_orientation(&mut buf_reader).unwrap_or(1);

    // 3. 解碼完整影像資料
    buf_reader.seek(SeekFrom::Start(0))?;
    let reader = ImageReader::new(buf_reader).with_guessed_format()?;
    let decoded = reader.decode()?;

    // 4. 依照 EXIF Orientation 校正方向
    Ok(apply_orientation(decoded, orientation))
}

/// 自記憶體位元組載入影像，預設安全上限為 64M 像素
pub fn load_image_from_memory(bytes: &[u8]) -> Result<DynamicImage, ImageError> {
    load_image_from_memory_with_limit(bytes, MAX_SAFE_PIXELS)
}

/// 自記憶體位元組載入影像，具備自訂最大像素保護上限與 EXIF Orientation 自動校正
pub fn load_image_from_memory_with_limit(
    bytes: &[u8],
    max_pixels: u64,
) -> Result<DynamicImage, ImageError> {
    let mut cursor = Cursor::new(bytes);

    // 1. 預檢尺寸
    let reader = ImageReader::new(&mut cursor).with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    check_pixel_limit(width, height, max_pixels)?;

    // 2. 嘗試讀取 EXIF Orientation
    cursor.seek(SeekFrom::Start(0))?;
    let orientation = read_orientation(&mut cursor).unwrap_or(1);

    // 3. 解碼完整影像資料
    cursor.seek(SeekFrom::Start(0))?;
    let reader = ImageReader::new(cursor).with_guessed_format()?;
    let decoded = reader.decode()?;

    // 4. 校正方向
    Ok(apply_orientation(decoded, orientation))
}

/// 同目錄暫存寫入，替換失敗直接保留原檔，不先移動或刪除目標。
pub fn write_atomic(
    target: &Path,
    encode: impl FnOnce(&mut BufWriter<&mut File>) -> Result<(), ImageError>,
) -> Result<(), ImageError> {
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp = Builder::new()
        .prefix(".image-output-")
        .tempfile_in(parent)?;
    {
        let mut writer = BufWriter::new(temp.as_file_mut());
        encode(&mut writer)?;
        writer.flush()?;
    }
    temp.as_file().sync_all()?;
    temp.persist(target)
        .map_err(|error| ImageError::Io(error.error))?;
    Ok(())
}

/// 舊有儲存入口維持格式支援；未知副檔名明確拒絕，JPEG 使用白色背景。
pub fn save_image_atomic<P: AsRef<Path>>(
    img: &DynamicImage,
    target_path: P,
) -> Result<(), ImageError> {
    let target = target_path.as_ref();
    let format = ImageFormat::from_path(target)
        .map_err(|_| ImageError::UnsupportedFormat(target.display().to_string()))?;
    if format == ImageFormat::Jpeg {
        return crate::output::save_with_options(
            &img.to_rgba8(),
            target,
            crate::output::OutputOptions {
                format: crate::output::OutputFormat::Jpeg,
                quality: 80,
                ..Default::default()
            },
        );
    }
    write_atomic(target, |writer| {
        img.write_to(writer, format)?;
        Ok(())
    })
}
