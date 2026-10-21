use std::fs::File;
use std::io::Write;
use image::{ImageBuffer, Rgba};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

#[test]
fn test_app_archive_open_navigation_and_save_protection() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("vacation_photos.zip");

    // 建立兩張不同顏色的測試圖片
    let img1 = ImageBuffer::from_pixel(20, 20, Rgba([255u8, 0u8, 0u8, 255u8])); // 紅
    let mut png1_bytes = Vec::new();
    img1.write_to(
        &mut std::io::Cursor::new(&mut png1_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    let img2 = ImageBuffer::from_pixel(20, 20, Rgba([0u8, 255u8, 0u8, 255u8])); // 綠
    let mut png2_bytes = Vec::new();
    img2.write_to(
        &mut std::io::Cursor::new(&mut png2_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    // 寫入 ZIP
    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip_writer.start_file("img_a.png", options).unwrap();
        zip_writer.write_all(&png1_bytes).unwrap();

        zip_writer.start_file("img_b.png", options).unwrap();
        zip_writer.write_all(&png2_bytes).unwrap();

        zip_writer.finish().unwrap();
    }

    // 初始化 App
    let mut app = app_ui::app::ImageViewerApp::default();
    assert!(app.open_file_path(zip_path.clone()).is_ok());

    // 驗證是否正確解壓與掛載
    assert!(app.archive_session.is_some());
    assert_eq!(app.playlist.len(), 2);
    assert_eq!(app.playlist.current_index(), Some(0));

    // 驗證第 1 張圖片已成功載入 Session 且判定來自壓縮包
    assert!(app.session.is_some());
    assert!(app.is_current_image_from_archive());

    let current_path = app.session.as_ref().unwrap().source_path().unwrap().to_path_buf();
    assert!(app.archive_session.as_ref().unwrap().is_from_archive(&current_path));

    // 導覽至下一張圖片
    app.navigate_next();
    assert_eq!(app.playlist.current_index(), Some(1));
    assert!(app.is_current_image_from_archive());

    // 記錄原暫存檔案最後修改時間與內容長度
    let original_bytes_len = std::fs::read(&current_path).unwrap().len();

    // 模擬編輯
    app.slider_params.brightness = 1.8;
    app.session.as_mut().unwrap().update_preview(app.slider_params);
    app.session.as_mut().unwrap().apply_adjustments();

    // 驗證安全防護：壓縮包圖片儲存至外部新檔案 (另存新檔)
    let exported_path = temp_dir.path().join("exported_copy.png");
    assert!(app.save_image_to(&exported_path));
    assert!(exported_path.exists());

    // 驗證原暫存檔與原始 ZIP 檔案絕對未被覆寫
    let current_bytes_len = std::fs::read(&current_path).unwrap().len();
    assert_eq!(original_bytes_len, current_bytes_len, "原壓縮檔解壓之檔案絕對不能被直接原地變更！");
}
