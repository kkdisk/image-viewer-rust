//! 大批 MD5 雜湊在有限工作執行緒堆疊上的回歸驗證。

use duplicate_finder::compute_md5;
use rayon::prelude::*;

#[test]
fn large_md5_scan_with_small_worker_stack() {
    const CHILD: &str = "IMAGE_VIEWER_MD5_STACK_TEST";
    if std::env::var_os(CHILD).is_none() {
        // 堆疊溢位會直接終止行程，使用子行程保留主測試的失敗報告。
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "large_md5_scan_with_small_worker_stack",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "大量 MD5 子行程失敗：{}；{}；{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let path = std::env::temp_dir().join(format!("md5-stack-{}.bin", std::process::id()));
    std::fs::write(&path, b"abc").unwrap();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .stack_size(if cfg!(debug_assertions) {
            2 * 1024 * 1024
        } else {
            128 * 1024
        })
        .build()
        .unwrap();
    let paths = vec![path.clone(); 63_198];
    let hashes: Vec<_> = pool.install(|| {
        paths
            .par_iter()
            .filter_map(|path| compute_md5(path).ok())
            .collect()
    });
    std::fs::remove_file(path).unwrap();
    assert_eq!(hashes.len(), paths.len());
    assert!(
        hashes
            .iter()
            .all(|hash| hash == "900150983cd24fb0d6963f7d28e17f72")
    );
}

#[test]
fn streaming_md5_preserves_digest_across_buffer_boundaries() {
    use md5::{Digest, Md5};

    let path = std::env::temp_dir().join(format!("md5-boundary-{}.bin", std::process::id()));
    for size in [0, 65_535, 65_536, 65_537, 131_089] {
        let bytes: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(
            compute_md5(&path).unwrap(),
            format!("{:x}", Md5::digest(&bytes))
        );
    }
    std::fs::remove_file(path).unwrap();
}
