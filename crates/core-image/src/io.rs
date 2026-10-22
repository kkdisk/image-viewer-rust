//! 影像安全載入與原子儲存模組

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Cursor, Seek, SeekFrom, Write};
use std::path::Path;

use image::{DynamicImage, ImageFormat, ImageReader};
use tempfile::Builder;

use crate::error::ImageError;
use crate::exif::{apply_orientation, read_orientation};

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

/// 原子儲存影像至目標路徑（先寫入同目錄暫存檔，寫入完成後原子替換）
pub fn save_image_atomic<P: AsRef<Path>>(
    img: &DynamicImage,
    target_path: P,
) -> Result<(), ImageError> {
    let target = target_path.as_ref();
    let parent = target.parent().unwrap_or_else(|| Path::new("."));

    // 由目標路徑推斷影像格式，未指定時預設為 PNG
    let format = ImageFormat::from_path(target).unwrap_or(ImageFormat::Png);

    let ext = target.extension().and_then(|s| s.to_str()).unwrap_or("tmp");

    // 在同目錄下建立具備唯一名稱的暫存檔案
    let mut temp_file = Builder::new()
        .suffix(&format!(".{ext}"))
        .tempfile_in(parent)?;

    // 寫入影像資料至暫存檔
    {
        let mut writer = BufWriter::new(&mut temp_file);
        img.write_to(&mut writer, format)?;
        writer.flush()?;
    }

    // 原子替換至目標路徑
    match temp_file.persist(target) {
        Ok(_) => Ok(()),
        Err(persist_err) => {
            // 在 Windows 環境下，若目標檔案已存在，部分情況下 persist 可能無法直接覆蓋
            // 使用備份交換方式安全替換，確保資料完整性
            let temp_path = persist_err.file.into_temp_path();
            if target.exists() {
                let backup_file = Builder::new()
                    .prefix(".atomic_backup_")
                    .tempfile_in(parent)?;
                let backup_path = backup_file.into_temp_path();

                // 1. 將舊檔移至備份
                fs::rename(target, &backup_path)?;
                // 2. 將新暫存檔移入目標
                if let Err(e) = fs::rename(&temp_path, target) {
                    // 若失敗，迅速復原舊檔案
                    let _ = fs::rename(&backup_path, target);
                    return Err(ImageError::Io(e));
                }
                // 3. 移除備份
                let _ = fs::remove_file(&backup_path);
                Ok(())
            } else {
                fs::rename(&temp_path, target).map_err(ImageError::Io)?;
                Ok(())
            }
        }
    }
}
