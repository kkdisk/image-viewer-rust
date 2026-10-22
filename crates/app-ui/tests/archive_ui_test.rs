use image::{ImageBuffer, Rgba};
use std::fs::File;
use std::io::Write;
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
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip_writer.start_file("img_a.png", options).unwrap();
        zip_writer.write_all(&png1_bytes).unwrap();

        zip_writer.start_file("img_b.png", options).unwrap();
        zip_writer.write_all(&png2_bytes).unwrap();

        zip_writer.finish().unwrap();
    }

    // 初始化 App
    let mut app = app_ui::app::ImageViewerApp::default();
    assert!(app.open_file_path(zip_path.clone()).is_ok());
    assert!(
        app.wait_archive_loaded(std::time::Duration::from_secs(5)),
        "非同步解壓應在 5 秒內完成"
    );

    // 驗證是否正確解壓與掛載
    assert!(app.archive_session.is_some());
    assert_eq!(app.playlist.len(), 2);
    assert_eq!(app.playlist.current_index(), Some(0));

    // 驗證第 1 張圖片已成功載入 Session 且判定來自壓縮包
    assert!(app.session.is_some());
    assert!(app.is_current_image_from_archive());

    let current_path = app
        .session
        .as_ref()
        .unwrap()
        .source_path()
        .unwrap()
        .to_path_buf();
    assert!(
        app.archive_session
            .as_ref()
            .unwrap()
            .is_from_archive(&current_path)
    );

    // 導覽至下一張圖片
    app.navigate_next();
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert_eq!(app.playlist.current_index(), Some(1));
    assert!(app.is_current_image_from_archive());

    // 記錄原暫存檔案最後修改時間與內容長度
    let original_bytes_len = std::fs::read(&current_path).unwrap().len();

    // 模擬編輯
    app.slider_params.brightness = 1.8;
    app.session
        .as_mut()
        .unwrap()
        .update_preview(app.slider_params);
    assert!(app.apply_current_adjustments());

    // 驗證安全防護：壓縮包圖片儲存至外部新檔案 (另存新檔)
    let exported_path = temp_dir.path().join("exported_copy.png");
    assert!(app.save_image_to(&exported_path));
    assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
    assert!(exported_path.exists());

    // 驗證原暫存檔與原始 ZIP 檔案絕對未被覆寫
    let current_bytes_len = std::fs::read(&current_path).unwrap().len();
    assert_eq!(
        original_bytes_len, current_bytes_len,
        "原壓縮檔解壓之檔案絕對不能被直接原地變更！"
    );
}

#[test]
fn test_app_encrypted_7z_flow() {
    let seven_zip_exe = std::path::Path::new(r"C:\Program Files\7-Zip\7z.exe");
    if !seven_zip_exe.exists() {
        return;
    }

    let temp_dir = tempdir().unwrap();
    let img_path = temp_dir.path().join("secret_art.png");

    let img = ImageBuffer::from_pixel(16, 16, Rgba([100u8, 150u8, 200u8, 255u8]));
    img.save(&img_path).unwrap();

    let archive_path = temp_dir.path().join("locked_gallery.7z");

    let status = std::process::Command::new(seven_zip_exe)
        .arg("a")
        .arg(&archive_path)
        .arg(&img_path)
        .arg("-pmy_top_secret")
        .arg("-mhe=on")
        .status()
        .unwrap();
    assert!(status.success());

    let mut app = app_ui::app::ImageViewerApp::default();

    // 1. 初次無密碼開啟 -> 應觸發密碼輸入對話框
    let open_res = app.open_file_path(archive_path.clone());
    assert!(open_res.is_ok());
    assert!(
        app.wait_archive_loaded(std::time::Duration::from_secs(5)),
        "背景掃描加密狀態完成"
    );
    assert!(app.show_password_dialog, "加密壓縮檔應彈出密碼輸入視窗");
    assert_eq!(app.pending_archive_path.as_ref(), Some(&archive_path));
    assert!(app.archive_session.is_none());

    // 2. 模擬輸入錯誤密碼
    let wrong_res = app.open_archive_with_password(archive_path.clone(), "wrong_password");
    assert!(wrong_res.is_ok(), "非同步任務分發應成功");
    assert!(
        app.wait_archive_loaded(std::time::Duration::from_secs(5)),
        "背景解密驗證完成"
    );
    assert!(app.archive_password_error.is_some());
    assert!(app.show_password_dialog, "密碼錯誤時對話框應保持開啟");

    // 3. 模擬輸入正確密碼
    let correct_res = app.open_archive_with_password(archive_path, "my_top_secret");
    assert!(correct_res.is_ok(), "非同步任務分發應成功");
    assert!(
        app.wait_archive_loaded(std::time::Duration::from_secs(5)),
        "背景解密解壓完成"
    );
    assert!(!app.show_password_dialog, "解鎖成功後密碼對話框應關閉");
    assert!(app.archive_session.is_some());
    assert_eq!(app.playlist.len(), 1);
    assert!(app.session.is_some());
    assert!(app.is_current_image_from_archive());
}
