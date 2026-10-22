//! duplicate-finder 單元與整合測試
//! 驗證：MD5 雜湊、pHash (DCT) 感知雜湊、Union-Find 分群與 1,000 筆資料效能基準 (< 100ms)

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use duplicate_finder::{
    DEFAULT_HASH_SIZE, UnionFind, batch_compute_hashes, compute_md5, compute_phash,
    compute_phash_from_image, find_exact_md5_groups, find_similar_groups, hamming_distance,
};
use image::{DynamicImage, ImageBuffer, Rgba};

#[test]
fn test_hamming_distance() {
    let a = vec![0b10101010, 0b11110000];
    let b = vec![0b10101011, 0b11110000]; // 差 1 bit
    assert_eq!(hamming_distance(&a, &b), 1);

    let c = vec![0b01010101, 0b00001111]; // 全相反，差 16 bits
    assert_eq!(hamming_distance(&a, &c), 16);

    let same = vec![0xAA, 0xBB];
    assert_eq!(hamming_distance(&same, &same), 0);
}

#[test]
fn test_union_find_clustering() {
    let mut uf = UnionFind::new(5);

    // 0 和 1 距離 2
    uf.union(0, 1, 2);
    // 1 和 2 距離 4
    uf.union(1, 2, 4);
    // 3 和 4 距離 1
    uf.union(3, 4, 1);

    // 0, 1, 2 應在同一集合，且該集合最大距離為 4
    let root0 = uf.find(0);
    assert_eq!(uf.find(1), root0);
    assert_eq!(uf.find(2), root0);
    assert_eq!(uf.max_distance(root0), 4);

    // 3, 4 應在另一集合，最大距離為 1
    let root3 = uf.find(3);
    assert_ne!(root0, root3);
    assert_eq!(uf.find(4), root3);
    assert_eq!(uf.max_distance(root3), 1);
}

#[test]
fn test_md5_and_phash_computation() {
    let temp_dir = std::env::temp_dir().join("duplicate_finder_hash_test");
    fs::create_dir_all(&temp_dir).unwrap();
    let img_path1 = temp_dir.join("test_img1.png");
    let img_path2 = temp_dir.join("test_img2.png");

    // 建立 32x32 純色與漸層圖片
    let img1 = ImageBuffer::from_pixel(32, 32, Rgba([120u8, 120u8, 120u8, 255u8]));
    img1.save(&img_path1).unwrap();

    // img2 內容與 img1 完全相同
    img1.save(&img_path2).unwrap();

    let md5_1 = compute_md5(&img_path1).unwrap();
    let md5_2 = compute_md5(&img_path2).unwrap();
    assert_eq!(md5_1, md5_2);
    assert_eq!(md5_1.len(), 32);

    let phash_1 = compute_phash(&img_path1, DEFAULT_HASH_SIZE).unwrap();
    let phash_2 = compute_phash(&img_path2, DEFAULT_HASH_SIZE).unwrap();
    assert_eq!(phash_1, phash_2);
    assert_eq!(hamming_distance(&phash_1, &phash_2), 0);

    // 批次平行計算測試
    let paths = vec![img_path1.clone(), img_path2.clone()];
    let batch_res = batch_compute_hashes(&paths, DEFAULT_HASH_SIZE);
    assert_eq!(batch_res.len(), 2);
    assert!(batch_res[0].1.is_ok());
    assert!(batch_res[1].1.is_ok());

    // MD5 精確分群測試
    let md5_entries = vec![(img_path1.clone(), md5_1), (img_path2.clone(), md5_2)];
    let exact_groups = find_exact_md5_groups(&md5_entries);
    assert_eq!(exact_groups.len(), 1);
    assert_eq!(exact_groups[0].items.len(), 2);
    assert_eq!(exact_groups[0].max_distance, 0);

    // 清理暫存檔
    let _ = fs::remove_file(img_path1);
    let _ = fs::remove_file(img_path2);
    let _ = fs::remove_dir(temp_dir);
}

#[test]
fn test_phash_perceptual_similarity() {
    // 建立兩張極度相似但有些微像素差異的圖片 (例如噪點或微幅亮度差異)
    let mut base_img = ImageBuffer::new(64, 64);
    for (x, y, pixel) in base_img.enumerate_pixels_mut() {
        let v = ((x + y) * 2) as u8;
        *pixel = Rgba([v, v, v, 255]);
    }

    let mut slightly_altered = base_img.clone();
    for pixel in slightly_altered.pixels_mut() {
        // 微調亮度 +1~2
        pixel.0[0] = pixel.0[0].saturating_add(2);
        pixel.0[1] = pixel.0[1].saturating_add(2);
        pixel.0[2] = pixel.0[2].saturating_add(2);
    }

    let dyn_base = DynamicImage::ImageRgba8(base_img);
    let dyn_alt = DynamicImage::ImageRgba8(slightly_altered);

    let h1 = compute_phash_from_image(&dyn_base, DEFAULT_HASH_SIZE);
    let h2 = compute_phash_from_image(&dyn_alt, DEFAULT_HASH_SIZE);

    let dist = hamming_distance(&h1, &h2);
    // 感知雜湊對全域微調應極為魯棒，Hamming 距離應很小 (<= 10)
    assert!(
        dist <= 10,
        "微調後圖片之 pHash Hamming distance 應 <= 10，實際為 {}",
        dist
    );

    let hash_entries = vec![
        (PathBuf::from("img_a.png"), h1),
        (PathBuf::from("img_b.png"), h2),
    ];

    let similar_groups = find_similar_groups(&hash_entries, 10);
    assert_eq!(similar_groups.len(), 1);
    assert_eq!(similar_groups[0].items.len(), 2);
    assert_eq!(similar_groups[0].max_distance, dist);
}

#[test]
fn test_1000_items_clustering_benchmark() {
    // 規格驗證：1,000 筆資料分群時間 < 100ms
    let mut synthetic_hashes: Vec<(PathBuf, Vec<u8>)> = Vec::with_capacity(1000);

    // 產生 1000 筆模擬 64-bit (8 bytes) 雜湊
    for i in 0..1000 {
        let path = PathBuf::from(format!("img_{:04}.jpg", i));
        // 每 10 張設定為幾乎相同的特徵（造成多個群組），其餘隨機分佈
        let group_id = (i / 10) as u8;
        let mut hash = vec![group_id; 8];
        if i % 10 != 0 {
            // 微擾動 1 bit
            hash[0] ^= 0b00000001;
        }
        synthetic_hashes.push((path, hash));
    }

    let start = Instant::now();
    let groups = find_similar_groups(&synthetic_hashes, 2);
    let elapsed = start.elapsed();

    println!(
        "1,000 筆資料分群完成，費時: {:?}, 找到相似群組數: {}",
        elapsed,
        groups.len()
    );

    assert!(
        elapsed.as_millis() < 100,
        "1,000 筆資料分群時間應 < 100ms，實際耗時: {:?}",
        elapsed
    );
    assert!(!groups.is_empty(), "應成功分群出相似圖片群組");
}
