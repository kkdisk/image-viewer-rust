use core_archive::{ArchiveError, ArchiveLimits, ManagedArchiveSession, sanitize_archive_path};
use image::{ImageBuffer, Rgba};
use std::fs::File;
use std::io::Write;
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
fn test_zip_archive_path_traversal_rejection() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("malicious.zip");

    let img = ImageBuffer::from_pixel(8, 8, Rgba([255u8, 0, 0, 255]));
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    // 建立帶有目錄跳躍名稱的惡意 ZIP 檔案
    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip_writer.start_file("../outside.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();
        zip_writer.finish().unwrap();
    }

    let res = ManagedArchiveSession::open(&zip_path);
    assert!(
        matches!(res, Err(ArchiveError::PathTraversal(_))),
        "包含 ../ 惡意路徑之 ZIP 必須被拒絕，實際為: {:?}",
        res.err()
    );
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
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

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
    assert!(
        !temp_path_to_check.exists(),
        "Session 結束後 temp_dir 必須被自動釋放清理"
    );
}

#[test]
fn test_zip_bomb_entry_limit_rejection() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("many_files.zip");

    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        for i in 0..5 {
            zip_writer
                .start_file(format!("file_{}.txt", i), options)
                .unwrap();
            zip_writer.write_all(b"data").unwrap();
        }
        zip_writer.finish().unwrap();
    }

    // 設定條目上限為 3，預期被 LimitExceeded 阻擋
    let limits = ArchiveLimits {
        max_entries: 3,
        max_entry_bytes: 1024 * 1024,
        max_total_bytes: 1024 * 1024,
    };

    let res = ManagedArchiveSession::open_with_password_and_limits(&zip_path, None, &limits);
    assert!(
        matches!(res, Err(ArchiveError::LimitExceeded(_))),
        "超出條目上限應返回 LimitExceeded，實際為: {:?}",
        res.err()
    );
}

#[test]
fn test_zip_bomb_file_size_limit_rejection() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("huge_file.zip");

    let img = ImageBuffer::from_pixel(16, 16, Rgba([255u8, 0, 0, 255]));
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip_writer.start_file("large_img.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();
        zip_writer.finish().unwrap();
    }

    // 設定單檔上限為 10 位元組（小於 png_bytes 實際大小）
    let limits = ArchiveLimits {
        max_entries: 100,
        max_entry_bytes: 10,
        max_total_bytes: 1024 * 1024,
    };

    let res = ManagedArchiveSession::open_with_password_and_limits(&zip_path, None, &limits);
    assert!(
        matches!(res, Err(ArchiveError::LimitExceeded(_))),
        "超出單檔解壓大小上限應返回 LimitExceeded，實際為: {:?}",
        res.err()
    );
}

#[test]
fn test_natural_sorting_order() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("numbered.zip");

    let img = ImageBuffer::from_pixel(4, 4, Rgba([0u8, 0, 255, 255]));
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        // 故意以混亂順序寫入: 10, 2, 1
        zip_writer.start_file("page_10.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();

        zip_writer.start_file("page_2.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();

        zip_writer.start_file("page_1.png", options).unwrap();
        zip_writer.write_all(&png_bytes).unwrap();

        zip_writer.finish().unwrap();
    }

    let session = ManagedArchiveSession::open(&zip_path).unwrap();
    assert_eq!(session.image_paths.len(), 3);

    let file_names: Vec<String> = session
        .image_paths
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
        .collect();

    // 自然排序應為 page_1.png, page_2.png, page_10.png
    assert_eq!(file_names, vec!["page_1.png", "page_2.png", "page_10.png"]);
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

#[test]
fn test_7z_with_password() {
    let seven_zip_exe = std::path::Path::new(r"C:\Program Files\7-Zip\7z.exe");
    if !seven_zip_exe.exists() {
        println!(
            "[跳過 7z 測試] 系統中未找到 7z.exe (路徑: {:?})",
            seven_zip_exe
        );
        return;
    }

    let temp_dir = tempdir().unwrap();
    let img_path = temp_dir.path().join("photo.png");

    let img = ImageBuffer::from_pixel(16, 16, Rgba([0u8, 255u8, 0u8, 255u8]));
    img.save(&img_path).unwrap();

    let archive_path = temp_dir.path().join("secure.7z");

    let status = std::process::Command::new(seven_zip_exe)
        .arg("a")
        .arg(&archive_path)
        .arg(&img_path)
        .arg("-prust_pass_456")
        .arg("-mhe=on")
        .status()
        .unwrap();
    assert!(status.success());

    // 1. 未提供密碼 -> 應回傳 PasswordRequired 或 InvalidPassword
    let res_no_pwd = ManagedArchiveSession::open(&archive_path);
    assert!(
        matches!(
            res_no_pwd,
            Err(ArchiveError::PasswordRequired | ArchiveError::InvalidPassword)
        ),
        "未提供密碼應回傳密碼要求，實際為: {:?}",
        res_no_pwd.err()
    );

    // 2. 錯誤密碼 -> 應回傳 InvalidPassword
    let res_wrong = ManagedArchiveSession::open_with_password(&archive_path, Some("wrong_pass"));
    assert!(
        matches!(
            res_wrong,
            Err(ArchiveError::InvalidPassword | ArchiveError::PasswordRequired)
        ),
        "錯誤密碼應回傳密碼錯誤，實際為: {:?}",
        res_wrong.err()
    );

    // 3. 正確密碼 -> 解壓成功
    let res_ok = ManagedArchiveSession::open_with_password(&archive_path, Some("rust_pass_456"));
    assert!(res_ok.is_ok(), "正確密碼解密失敗: {:?}", res_ok.err());
    let session = res_ok.unwrap();
    assert_eq!(session.image_paths.len(), 1);
    assert!(session.image_paths[0].exists());
}

#[test]
fn test_7z_solid_archive_stream_alignment_regression() {
    let seven_zip_exe = std::path::Path::new(r"C:\Program Files\7-Zip\7z.exe");
    if !seven_zip_exe.exists() {
        println!("[跳過 7z 固實壓縮回歸測試] 系統中未找到 7z.exe");
        return;
    }

    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("files");
    std::fs::create_dir(&src_dir).unwrap();

    // 建立交錯的檔案結構：photo1.png -> dummy.txt -> photo2.png
    let img = ImageBuffer::from_pixel(8, 8, Rgba([255u8, 128, 0, 255]));
    img.save(src_dir.join("01_photo.png")).unwrap();
    std::fs::write(src_dir.join("02_readme.txt"), b"Solid block middle text").unwrap();
    img.save(src_dir.join("03_photo.png")).unwrap();

    let archive_path = temp_dir.path().join("solid_archive.7z");

    // 建立開啟固實壓縮 (-ms=on) 的 7z 檔案
    let status = std::process::Command::new(seven_zip_exe)
        .arg("a")
        .arg(&archive_path)
        .arg(format!("{}\\*", src_dir.display()))
        .arg("-ms=on")
        .status()
        .unwrap();
    assert!(status.success());

    // 驗證解壓：固實串流對齊修復後，跳過非圖片 entry 不應造成 CRC mismatch
    let session =
        ManagedArchiveSession::open(&archive_path).expect("固實壓縮交錯非圖片條目解壓應成功");
    assert_eq!(session.image_paths.len(), 2);
    assert!(session.image_paths[0].ends_with("01_photo.png"));
    assert!(session.image_paths[1].ends_with("03_photo.png"));
}

#[test]
fn test_archive_progress_callback_reporting() {
    let temp_dir = tempdir().unwrap();
    let zip_path = temp_dir.path().join("progress_test.zip");

    let img = ImageBuffer::from_pixel(8, 8, Rgba([0u8, 255, 0, 255]));
    let mut png_bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    )
    .unwrap();

    // 建立 3 個圖片檔案的 ZIP
    {
        let file = File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        for i in 1..=3 {
            zip_writer
                .start_file(format!("img_{:02}.png", i), options)
                .unwrap();
            zip_writer.write_all(&png_bytes).unwrap();
        }
        zip_writer.finish().unwrap();
    }

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let call_count = Arc::new(AtomicUsize::new(0));
    let last_index = Arc::new(AtomicUsize::new(0));
    let cc = Arc::clone(&call_count);
    let li = Arc::clone(&last_index);

    let progress_cb = Arc::new(move |prog: core_archive::ArchiveProgress| {
        cc.fetch_add(1, Ordering::SeqCst);
        li.store(prog.current_entry_index, Ordering::SeqCst);
        assert_eq!(prog.total_entries, 3);
    });

    let session = ManagedArchiveSession::open_with_progress_and_limits(
        &zip_path,
        None,
        &ArchiveLimits::default(),
        Some(progress_cb),
    )
    .expect("帶有進度回呼的解壓應成功");

    assert_eq!(session.image_paths.len(), 3);
    assert_eq!(call_count.load(Ordering::SeqCst), 3);
    assert_eq!(last_index.load(Ordering::SeqCst), 3);
}
