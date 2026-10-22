use app_ui::panels::{DuplicateFinderState, DuplicateGroupUi, DuplicateItemUi};
use image::{ImageBuffer, Rgba};
use std::fs;

#[test]
fn scan_delivers_metadata_failures_and_isolates_replaced_scan() {
    use app_ui::panels::{DuplicateMode, duplicate_dialog::ScanStatus};
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good.png");
    ImageBuffer::from_pixel(8, 8, Rgba([100u8, 50, 50, 255]))
        .save(&good)
        .unwrap();
    fs::copy(&good, dir.path().join("copy.png")).unwrap();
    fs::write(dir.path().join("bad.png"), b"invalid").unwrap();
    let mut state = DuplicateFinderState::default();
    state.target_dir = Some(dir.path().to_path_buf());
    state.mode = DuplicateMode::SimilarPhash;
    state.start_scan();
    let start = Instant::now();
    loop {
        state.update_progress();
        if matches!(state.status, ScanStatus::Completed { .. }) {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(
        state.status,
        ScanStatus::Completed {
            total_scanned: 3,
            succeeded: 2,
            failed: 1
        }
    ));
    assert_eq!(state.groups.len(), 1);
    assert!(
        state.groups[0]
            .items
            .iter()
            .all(|item| item.dimensions == Some((8, 8)) && item.size_bytes > 0)
    );
    state.start_scan();
    let empty = tempfile::tempdir().unwrap();
    state.target_dir = Some(empty.path().to_path_buf());
    state.start_scan();
    let start = Instant::now();
    loop {
        state.update_progress();
        if matches!(
            state.status,
            ScanStatus::Completed {
                total_scanned: 0,
                ..
            }
        ) {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(10));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(state.groups.is_empty());
}

#[test]
fn smart_selection_keeps_unknown_sizes_and_a_valid_representative() {
    let mut state = DuplicateFinderState::default();
    let item = |dimensions, size_bytes| DuplicateItemUi {
        path: "test.png".into(),
        dimensions,
        size_bytes,
        selected_for_delete: true,
    };
    state.groups = vec![DuplicateGroupUi {
        id: 1,
        max_distance: 0,
        is_expanded: true,
        items: vec![
            item(None, u64::MAX),
            item(Some((32, 32)), 10),
            item(Some((64, 64)), 1),
            item(Some((0, 100)), u64::MAX),
        ],
    }];
    state.mark_all_duplicates_keep_highest_resolution();
    let selected: Vec<_> = state.groups[0]
        .items
        .iter()
        .map(|i| i.selected_for_delete)
        .collect();
    assert_eq!(selected, vec![false, true, false, false]);
    state.groups[0].items = vec![item(None, 100), item(Some((0, 0)), 200)];
    state.mark_all_duplicates_keep_highest_resolution();
    assert!(state.groups[0].items.iter().all(|i| !i.selected_for_delete));
}

#[test]
fn test_duplicate_finder_state_selection_and_deletion() {
    let temp_dir = std::env::temp_dir().join("image_viewer_test_duplicates");
    fs::create_dir_all(&temp_dir).unwrap();

    let path_a1 = temp_dir.join("dup_a1.png");
    let path_a2 = temp_dir.join("dup_a2.png");
    let path_a3 = temp_dir.join("dup_a3.png");

    // 建立測試圖片
    let img_small = ImageBuffer::from_pixel(32, 32, Rgba([200u8, 50u8, 50u8, 255u8]));
    let img_large = ImageBuffer::from_pixel(64, 64, Rgba([200u8, 50u8, 50u8, 255u8]));

    img_small.save(&path_a1).unwrap();
    img_small.save(&path_a2).unwrap();
    img_large.save(&path_a3).unwrap();

    let mut state = DuplicateFinderState::default();
    state.open_with_dir(Some(temp_dir.clone()));
    assert_eq!(state.target_dir, Some(temp_dir.clone()));

    // 模擬已載入之重複分群資料
    let item1 = DuplicateItemUi {
        path: path_a1.clone(),
        size_bytes: fs::metadata(&path_a1).unwrap().len(),
        dimensions: Some((32, 32)),
        selected_for_delete: false,
    };
    let item2 = DuplicateItemUi {
        path: path_a2.clone(),
        size_bytes: fs::metadata(&path_a2).unwrap().len(),
        dimensions: Some((32, 32)),
        selected_for_delete: false,
    };
    let item3 = DuplicateItemUi {
        path: path_a3.clone(),
        size_bytes: fs::metadata(&path_a3).unwrap().len(),
        dimensions: Some((64, 64)),
        selected_for_delete: false,
    };

    state.groups = vec![DuplicateGroupUi {
        id: 1,
        max_distance: 0,
        is_expanded: true,
        items: vec![item1, item2, item3],
    }];

    // 1. 測試「保留每組第一張」
    state.mark_all_duplicates_except_first();
    assert!(!state.groups[0].items[0].selected_for_delete);
    assert!(state.groups[0].items[1].selected_for_delete);
    assert!(state.groups[0].items[2].selected_for_delete);
    let (count, _) = state.selected_delete_stats();
    assert_eq!(count, 2);

    // 2. 測試「保留最高解析度 (item3: 64x64)」
    state.mark_all_duplicates_keep_highest_resolution();
    assert!(state.groups[0].items[0].selected_for_delete);
    assert!(state.groups[0].items[1].selected_for_delete);
    assert!(!state.groups[0].items[2].selected_for_delete);

    // 3. 測試「全部取消勾選」
    state.uncheck_all();
    let (count, _) = state.selected_delete_stats();
    assert_eq!(count, 0);

    // 4. 測試安全移至系統資源回收桶
    state.groups[0].items[1].selected_for_delete = true; // 僅刪除 a2
    state.execute_safe_delete();

    // 驗證 a2 在原目錄已被移除 (移至回收桶)
    assert!(!path_a2.exists());
    // a1 與 a3 仍然完好
    assert!(path_a1.exists());
    assert!(path_a3.exists());

    // 驗證群組中剩下 a1 與 a3
    assert_eq!(state.groups.len(), 1);
    assert_eq!(state.groups[0].items.len(), 2);
    assert_eq!(state.groups[0].items[0].path, path_a1);
    assert_eq!(state.groups[0].items[1].path, path_a3);

    // 清理殘留測試檔案
    let _ = fs::remove_file(path_a1);
    let _ = fs::remove_file(path_a3);
    let _ = fs::remove_dir(temp_dir);
}
