//! Playlist 目錄清單與導覽邏輯單元測試

use std::fs::File;
use tempfile::tempdir;

#[test]
fn test_image_extensions_detection() {
    let dir = tempdir().expect("建立暫存目錄失敗");

    // 建立數個測試檔案
    let files = ["pic_b.jpg", "pic_a.PNG", "not_image.txt", "pic_c.webp"];
    for name in &files {
        File::create(dir.path().join(name)).unwrap();
    }

    // 驗證副檔名過濾
    let mut image_names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| {
                    let lower = ext.to_lowercase();
                    ["png", "jpg", "jpeg", "bmp", "webp", "tiff", "gif"].contains(&lower.as_str())
                })
                .unwrap_or(false)
        })
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    image_names.sort();
    assert_eq!(image_names.len(), 3);
    assert!(!image_names.contains(&"not_image.txt".to_string()));
}
