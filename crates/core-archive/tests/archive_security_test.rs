use std::fs::File;
use std::io::Write;
use core_archive::{
    sanitize_archive_path, ArchiveError, ManagedArchiveSession,
};
use image::{ImageBuffer, Rgba};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

#[test]
fn test_path_traversal_sanitization() {
    // 正常路徑
    assert!(sanitize_archive_path("images/photo.png").is_ok());
    assert!(sanitize_archive_path("nested/deep/cat.jpg").is_ok());

    // 惡意路徑（跳出目錄）
    assert!(matches!(
        sanitize_archive_path("../evil.png"),
        Err(ArchiveError::PathTraversal(_))
    ));
    assert!(matches!(
        sanitize_archive_path("folder/../../escape.jpg"),
        Err(ArchiveError::PathTraversal(_))
    ));
    assert!(matches!(
        sanitize_archive_path("/absolute/path.png"),
        Err(ArchiveError::PathTraversal(_))
    ));
    assert!(matches!(
        sanitize_archive_path(r"C:\Windows\system32\cmd.exe"),
        Err(ArchiveError::PathTraversal(_))
    ));
}

#[test]
fn test_zip_archive_safe_extraction_and_lifecycle() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("test_album.zip");

    // 建立測試圖片
    let img = ImageBuffer::from_pixel(16, 16, Rgba([255u8, 0u8, 0u8, 255u8]));
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    // 建立測試 ZIP 檔（包含 2 張圖片 + 1 個文字檔）
    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        // 圖片 1
        zip_writer.start_file("photo1.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();

        // 圖片 2 (子目錄中)
        zip_writer.start_file("subdir/photo2.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();

        // 非圖片檔案 (應被忽略)
        zip_writer.start_file("readme.txt", options).unwrap();
        zip_writer.write_all(b"Hello world").unwrap();

        zip_writer.finish().unwrap();
    }

    // 測試開啟並解壓
    let temp_path_to_check;
    {
        let session = ManagedArchiveSession::open(&zip_path).unwrap();
        assert_eq!(session.image_paths.len(), 2);
        assert!(session.image_paths.iter().all(|p| p.exists()));

        // 驗證 is_from_archive
        let extracted_img = &session.image_paths[0];
        assert!(session.is_from_archive(extracted_img));
        assert!(!session.is_from_archive(temp_dir.path().join("external.png")));

        temp_path_to_check = session.temp_dir.path().to_path_buf();
    }

    // 驗證 session 結束後暫存目錄已被自動清理釋放
    assert!(!temp_path_to_check.exists(), "Session 結束後 temp_dir 必須被自動釋放清理");
}

#[test]
fn test_archive_with_no_images_fails() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("empty_album.zip");

    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip_writer.start_file("document.pdf", options).unwrap();
        zip_writer.write_all(b"PDF content").unwrap();

        zip_writer.finish().unwrap();
    }

    let res = ManagedArchiveSession::open(&zip_path);
    assert!(matches!(res, Err(ArchiveError::NoImagesFound)));
}
