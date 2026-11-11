//! 可讀與可寫格式的單一來源。
//!
//! 可讀清單決定瀏覽、查重與開檔篩選；可寫清單決定另存篩選。兩者刻意分開，
//! 只能解碼、不能編碼的格式不得出現在另存選項中。

use std::path::Path;

/// 可解碼的圖片副檔名（小寫）。
pub const READABLE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "webp", "tiff", "tif", "gif", "heic", "heif", "avif",
];

/// 只能解碼、無法寫出的格式：來源為此類時不得原地儲存，目標為此類時拒絕。
pub const READ_ONLY_EXTENSIONS: &[&str] = &["heic", "heif", "avif"];

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

/// 是否為只能讀取的格式（依副檔名）。
pub fn is_read_only_extension(path: &Path) -> bool {
    lower_extension(path).is_some_and(|ext| READ_ONLY_EXTENSIONS.contains(&ext.as_str()))
}

/// 唯讀來源的預設另存檔名：同名 `.jpg`。
pub fn read_only_save_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名".into());
    format!("{stem}.jpg")
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
        for ext in READ_ONLY_EXTENSIONS {
            assert!(READABLE_EXTENSIONS.contains(ext));
        }
        assert!(is_read_only_extension(Path::new("IMG_0001.HEIC")));
        // GIF 可原地儲存（動畫轉靜態），不屬於唯讀格式。
        assert!(!is_read_only_extension(Path::new("動畫.gif")));
        assert_eq!(
            read_only_save_name(Path::new("D:/相簿/IMG_0001.HEIC")),
            "IMG_0001.jpg"
        );
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
