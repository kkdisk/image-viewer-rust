//! 跨套件格式清單一致性：core-archive 依主規格不依賴 core-image，
//! 由 app-ui 這一層確保兩份清單不會不同步。
use std::collections::BTreeSet;

#[test]
fn 壓縮檔內圖片副檔名與可讀格式清單一致() {
    let archive: BTreeSet<&str> = core_archive::SUPPORTED_IMAGE_EXTENSIONS
        .iter()
        .copied()
        .collect();
    let readable: BTreeSet<&str> = core_image::formats::READABLE_EXTENSIONS
        .iter()
        .copied()
        .collect();
    assert_eq!(archive, readable);
}
