//! EXIF 元資料讀取與 Orientation 方向自動校正

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Seek, SeekFrom};
use std::path::Path;

use exif::{In, Reader, Tag, Value};
use image::DynamicImage;

/// 原始檔案的唯讀拍攝資訊；不將檔案修改時間當作拍攝時間。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhotoMetadata {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub captured_at: Option<String>,
    pub datetime_source: Option<&'static str>,
    pub timezone: Option<String>,
    pub orientation: Option<u32>,
}

impl PhotoMetadata {
    /// 型號若已包含品牌前綴，避免同一品牌重複顯示。
    pub fn camera_label(&self) -> Option<String> {
        match (&self.camera_make, &self.camera_model) {
            (Some(make), Some(model))
                if model
                    .get(..make.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(make))
                    && model
                        .as_bytes()
                        .get(make.len())
                        .is_none_or(u8::is_ascii_whitespace) =>
            {
                Some(model.clone())
            }
            (Some(make), Some(model)) => Some(format!("{make} {model}")),
            (Some(value), None) | (None, Some(value)) => Some(value.clone()),
            (None, None) => None,
        }
    }
    pub fn orientation_label(&self) -> &'static str {
        match self.orientation {
            Some(1) => "正常方向",
            Some(2) => "水平鏡像，載入時自動校正",
            Some(3) => "旋轉 180 度，載入時自動校正",
            Some(4) => "垂直鏡像，載入時自動校正",
            Some(5) => "轉置，載入時自動校正",
            Some(6) => "順時針 90 度，載入時自動校正",
            Some(7) => "反轉置，載入時自動校正",
            Some(8) => "逆時針 90 度，載入時自動校正",
            _ => "未記錄有效方向，維持像素原方向",
        }
    }
}

pub fn read_photo_metadata(path: &Path) -> PhotoMetadata {
    let Ok(file) = File::open(path) else {
        return PhotoMetadata::default();
    };
    read_photo_metadata_from_reader(&mut BufReader::new(file))
}

pub fn read_photo_metadata_from_reader<R: BufRead + Seek>(reader: &mut R) -> PhotoMetadata {
    let _ = reader.seek(SeekFrom::Start(0));
    let Ok(data) = Reader::new().read_from_container(reader) else {
        return PhotoMetadata::default();
    };
    let ascii = |tag| -> Option<String> {
        let Value::Ascii(values) = &data.get_field(tag, In::PRIMARY)?.value else {
            return None;
        };
        let text = std::str::from_utf8(values.first()?)
            .ok()?
            .trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
        (!text.is_empty() && text.len() <= 64 && !text.chars().any(char::is_control))
            .then(|| text.to_owned())
    };
    let orientation = data
        .get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
        .filter(|v| (1..=8).contains(v));
    let mut result = PhotoMetadata {
        camera_make: ascii(Tag::Make),
        camera_model: ascii(Tag::Model),
        orientation,
        ..Default::default()
    };
    for (date, fraction, offset, label) in [
        (
            Tag::DateTimeOriginal,
            Tag::SubSecTimeOriginal,
            Tag::OffsetTimeOriginal,
            "拍攝時間",
        ),
        (
            Tag::DateTimeDigitized,
            Tag::SubSecTimeDigitized,
            Tag::OffsetTimeDigitized,
            "數位化時間（未記錄拍攝時間）",
        ),
        (
            Tag::DateTime,
            Tag::SubSecTime,
            Tag::OffsetTime,
            "EXIF 修改時間（未記錄拍攝時間）",
        ),
    ] {
        let Some(raw) = ascii(date) else {
            continue;
        };
        let Some(mut formatted) = format_exif_datetime(&raw) else {
            continue;
        };
        if let Some(digits) =
            ascii(fraction).filter(|v| v.len() <= 9 && v.bytes().all(|c| c.is_ascii_digit()))
        {
            formatted.push('.');
            formatted.push_str(&digits);
        }
        result.timezone = ascii(offset).filter(|v| {
            let b = v.as_bytes();
            b.len() == 6
                && matches!(b[0], b'+' | b'-')
                && b[3] == b':'
                && v[1..3].parse::<u8>().is_ok_and(|h| h <= 23)
                && v[4..6].parse::<u8>().is_ok_and(|m| m <= 59)
        });
        result.captured_at = Some(formatted);
        result.datetime_source = Some(label);
        break;
    }
    result
}

fn format_exif_datetime(value: &str) -> Option<String> {
    let b = value.as_bytes();
    if b.len() != 19
        || b[4] != b':'
        || b[7] != b':'
        || b[10] != b' '
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16) || c.is_ascii_digit())
    {
        return None;
    }
    let year = value[0..4].parse::<u32>().ok()?;
    let month = value[5..7].parse::<u32>().ok()?;
    let day = value[8..10].parse::<u32>().ok()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if year == 0
        || day == 0
        || day > days
        || value[11..13].parse::<u8>().ok()? > 23
        || value[14..16].parse::<u8>().ok()? > 59
        || value[17..19].parse::<u8>().ok()? > 60
    {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}",
        &value[0..4],
        &value[5..7],
        &value[8..10],
        &value[11..19]
    ))
}

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
