//! EXIF 元資料讀取與 Orientation 方向自動校正

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Seek, SeekFrom};
use std::path::Path;

use exif::{In, Reader, Tag, Value};
use image::DynamicImage;

/// 自輸入流中嘗試讀取 EXIF Orientation 標籤（數值通常介於 1 至 8）
///
/// 若無 EXIF 資訊、格式非容器支援或無 Orientation 標籤，則返回 `None`。
pub fn read_orientation<R: BufRead + Seek>(reader: &mut R) -> Option<u32> {
    // 確保 reader 從開頭讀取
    let _ = reader.seek(SeekFrom::Start(0));

    let exif_reader = Reader::new();
    let exif_data = exif_reader.read_from_container(reader).ok()?;

    let orientation_field = exif_data.get_field(Tag::Orientation, In::PRIMARY)?;
    match orientation_field.value {
        Value::Short(ref vec) if !vec.is_empty() => Some(vec[0] as u32),
        Value::Long(ref vec) if !vec.is_empty() => Some(vec[0]),
        Value::Byte(ref vec) if !vec.is_empty() => Some(vec[0] as u32),
        _ => None,
    }
}

/// 自指定檔案路徑讀取 EXIF Orientation 標籤
pub fn read_orientation_from_file<P: AsRef<Path>>(path: P) -> Option<u32> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    read_orientation(&mut reader)
}

/// 自記憶體二進位資料中讀取 EXIF Orientation 標籤
pub fn read_orientation_from_bytes(bytes: &[u8]) -> Option<u32> {
    let mut cursor = Cursor::new(bytes);
    read_orientation(&mut cursor)
}

/// 依據 EXIF Orientation 標籤值校正影像方向
///
/// 各 Orientation 標籤校正邏輯：
/// - 1: 正常（未變更）
/// - 2: 水平翻轉
/// - 3: 旋轉 180 度
/// - 4: 垂直翻轉
/// - 5: 水平翻轉後逆時針旋轉 90 度 (Transposed)
/// - 6: 順時針旋轉 90 度
/// - 7: 水平翻轉後順時針旋轉 90 度 (Transverse)
/// - 8: 逆時針旋轉 90 度 (順時針 270 度)
/// - 其他未定義數值：保持原圖
pub fn apply_orientation(img: DynamicImage, orientation: u32) -> DynamicImage {
    match orientation {
        1 => img,
        2 => img.fliph(),
        3 => img.rotate180(),
        4 => img.flipv(),
        5 => img.fliph().rotate270(),
        6 => img.rotate90(),
        7 => img.fliph().rotate90(),
        8 => img.rotate270(),
        _ => img,
    }
}
