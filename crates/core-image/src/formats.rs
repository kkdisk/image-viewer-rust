//! 可讀與可寫格式的單一來源。
//!
//! 可讀清單決定瀏覽、查重與開檔篩選；可寫清單決定另存篩選。兩者刻意分開，
//! 只能解碼、不能編碼的格式不得出現在另存選項中。

use std::path::Path;

/// 可解碼的圖片副檔名（小寫）。
pub const READABLE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "webp", "tiff", "tif", "gif", "heic", "heif", "avif",
];

/// 另存對話框的格式篩選（顯示名稱、副檔名），順序即對話框順序。
pub const WRITABLE_FILTERS: &[(&str, &[&str])] = &[
    ("PNG 影像 (*.png)", &["png"]),
    ("JPEG 影像 (*.jpg, *.jpeg)", &["jpg", "jpeg"]),
    ("BMP 影像 (*.bmp)", &["bmp"]),
    ("WebP 影像 (*.webp)", &["webp"]),
    ("TIFF 影像 (*.tiff)", &["tiff"]),
];

fn lower_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
}

/// 依副檔名判斷是否為可讀圖片。
pub fn is_readable_extension(path: &Path) -> bool {
    lower_extension(path).is_some_and(|ext| READABLE_EXTENSIONS.contains(&ext.as_str()))
}

/// 依副檔名判斷是否為可寫圖片。
pub fn is_writable_extension(path: &Path) -> bool {
    lower_extension(path).is_some_and(|ext| {
        WRITABLE_FILTERS
            .iter()
            .any(|(_, extensions)| extensions.contains(&ext.as_str()))
    })
}

/// 產生「(*.png, *.jpg, …)」形式的篩選說明。
pub fn filter_label(name: &str, extensions: &[&str]) -> String {
    let patterns: Vec<String> = extensions.iter().map(|ext| format!("*.{ext}")).collect();
    format!("{name} ({})", patterns.join(", "))
}

/// 資訊面板用的格式描述：格式名稱與色彩處理說明（sRGB 或非 HEIF 時為 `None`）。
pub fn describe(path: &Path) -> (String, Option<String>) {
    if crate::heif::is_heif_path(path) {
        let mut reader = match std::fs::File::open(path) {
            Ok(file) => std::io::BufReader::new(file),
            Err(_) => return ("未知".into(), None),
        };
        return match crate::heif::probe::probe(&mut reader) {
            Ok(info) => (
                crate::heif::format_label(&info),
                crate::heif::color_note(&info),
            ),
            Err(_) => ("HEIF（容器損毀）".into(), None),
        };
    }
    let format = image::ImageReader::open(path)
        .ok()
        .and_then(|reader| reader.with_guessed_format().ok())
        .and_then(|reader| reader.format())
        .map_or_else(|| "未知".into(), |format| format!("{format:?}"));
    (format, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 只能解碼的_heif_家族不得出現在另存格式() {
        for ext in ["heic", "heif", "avif"] {
            assert!(READABLE_EXTENSIONS.contains(&ext));
            assert!(!is_writable_extension(Path::new(&format!("相片.{ext}"))));
        }
        assert!(is_readable_extension(Path::new("IMG_0001.HEIC")));
    }

    #[test]
    fn 可寫格式皆可讀且判斷不分大小寫() {
        for (_, extensions) in WRITABLE_FILTERS {
            for ext in *extensions {
                assert!(READABLE_EXTENSIONS.contains(ext), "{ext} 可寫但不可讀");
            }
        }
        assert!(is_readable_extension(Path::new("相片.JPG")));
        assert!(is_writable_extension(Path::new("輸出.Tiff")));
        assert!(!is_writable_extension(Path::new("動畫.gif")));
        assert!(!is_readable_extension(Path::new("無副檔名")));
        assert_eq!(filter_label("圖片", &["png", "jpg"]), "圖片 (*.png, *.jpg)");
    }
}
