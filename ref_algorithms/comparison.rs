/// 相似度比對模組 — Union-Find 分組與 Hamming Distance
///
/// 對大量圖片的 pHash 做 O(n²) 配對比較，使用 Union-Find 合併相似圖片。
/// 1000 張圖片時：Python ~5 秒 → Rust ~0.05 秒。

use pyo3::prelude::*;

/// 計算兩個 hash bytes 之間的 Hamming distance。
///
/// Args:
///     hash_a: 第一個 hash bytes。
///     hash_b: 第二個 hash bytes。
///
/// Returns:
///     Hamming distance（不同 bit 的數量）。
#[pyfunction]
pub fn hamming_distance(hash_a: Vec<u8>, hash_b: Vec<u8>) -> PyResult<u32> {
    Ok(hamming_distance_internal(&hash_a, &hash_b))
}

/// 從一組 pHash 值中，找出相似的圖片群組。
///
/// 使用 Union-Find（路徑壓縮 + 按秩合併）進行 O(n²) 配對比較。
///
/// Args:
///     hashes: 每張圖片的 hash bytes 清單。
///     threshold: Hamming distance 門檻（距離 <= threshold 視為相似）。
///
/// Returns:
///     群組索引清單：每組至少 2 張圖片，例如 [[0, 3, 7], [1, 5]]。
///     每組內還附帶該組的最大 Hamming distance。
///     回傳格式：Vec<(Vec<usize>, u32)> — (群組索引, 最大距離)。
#[pyfunction]
pub fn find_similar_groups(
    hashes: Vec<Vec<u8>>,
    threshold: u32,
) -> PyResult<Vec<(Vec<usize>, u32)>> {
    let n = hashes.len();
    if n < 2 {
        return Ok(Vec::new());
    }

    // 初始化 Union-Find
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank: Vec<u32> = vec![0; n];
    let mut max_dist: Vec<u32> = vec![0; n];

    // O(n²) 配對比較
    for i in 0..n {
        if hashes[i].is_empty() {
            continue;
        }
        for j in (i + 1)..n {
            if hashes[j].is_empty() {
                continue;
            }
            let dist = hamming_distance_internal(&hashes[i], &hashes[j]);
            if dist <= threshold {
                union(&mut parent, &mut rank, &mut max_dist, i, j, dist);
            }
        }
    }

    // 收集群組
    let mut group_map: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        group_map.entry(root).or_default().push(i);
    }

    // 過濾：只保留至少 2 張圖片的群組
    let result: Vec<(Vec<usize>, u32)> = group_map
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(root, members)| {
            let dist = max_dist[root];
            (members, dist)
        })
        .collect();

    Ok(result)
}

// ============================================================================
// 內部輔助函式
// ============================================================================

/// Hamming distance：計算兩個 byte 陣列之間不同 bit 的數量
fn hamming_distance_internal(a: &[u8], b: &[u8]) -> u32 {
    let min_len = a.len().min(b.len());
    let max_len = a.len().max(b.len());
    let mut dist: u32 = 0;

    // 比較重疊部分
    for i in 0..min_len {
        dist += (a[i] ^ b[i]).count_ones();
    }

    // 多餘的 bytes 視為全不同
    // （長度不一致時，多出來的 byte 中的每個 bit 都視為差異）
    let extra_bytes = (max_len - min_len) as u32;
    dist += extra_bytes * 8;

    dist
}

/// Union-Find: find（路徑壓縮）
fn find(parent: &mut [usize], x: usize) -> usize {
    if parent[x] != x {
        parent[x] = find(parent, parent[x]);
    }
    parent[x]
}

/// Union-Find: union（按秩合併 + 追蹤最大距離）
fn union(
    parent: &mut [usize],
    rank: &mut [u32],
    max_dist: &mut [u32],
    x: usize,
    y: usize,
    dist: u32,
) {
    let rx = find(parent, x);
    let ry = find(parent, y);
    if rx == ry {
        // 已在同一群組，但仍需更新最大距離
        max_dist[rx] = max_dist[rx].max(dist);
        return;
    }

    // 按秩合併
    let (new_root, old_root) = if rank[rx] < rank[ry] {
        (ry, rx)
    } else if rank[rx] > rank[ry] {
        (rx, ry)
    } else {
        rank[rx] += 1;
        (rx, ry)
    };

    parent[old_root] = new_root;
    max_dist[new_root] = max_dist[new_root]
        .max(max_dist[old_root])
        .max(dist);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hamming_distance_identical() {
        let a = vec![0xFF, 0x00];
        let b = vec![0xFF, 0x00];
        assert_eq!(hamming_distance_internal(&a, &b), 0);
    }

    #[test]
    fn test_hamming_distance_all_different() {
        let a = vec![0xFF];
        let b = vec![0x00];
        assert_eq!(hamming_distance_internal(&a, &b), 8);
    }

    #[test]
    fn test_hamming_distance_one_bit() {
        let a = vec![0b11111111];
        let b = vec![0b11111110];
        assert_eq!(hamming_distance_internal(&a, &b), 1);
    }

    #[test]
    fn test_hamming_distance_different_lengths() {
        let a = vec![0xFF, 0xFF];
        let b = vec![0xFF];
        // 第一個 byte 相同 (0 diff)，第二個 byte 視為全差 (8 diff)
        assert_eq!(hamming_distance_internal(&a, &b), 8);
    }

    #[test]
    fn test_find_similar_groups_basic() {
        // 3 個 hash：a==b，c 不同
        let hashes = vec![
            vec![0xFF, 0x00],
            vec![0xFF, 0x00], // 和 0 完全相同
            vec![0x00, 0xFF], // 距離 = 16
        ];
        let groups = find_similar_groups(hashes, 5).unwrap();
        assert_eq!(groups.len(), 1); // 只有 {0, 1} 一組
        let (members, dist) = &groups[0];
        assert!(members.contains(&0));
        assert!(members.contains(&1));
        assert_eq!(*dist, 0);
    }

    #[test]
    fn test_find_similar_groups_no_duplicates() {
        let hashes = vec![vec![0x00], vec![0xFF]];
        let groups = find_similar_groups(hashes, 2).unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn test_find_similar_groups_transitive() {
        // a 和 b 相似（距離=2），b 和 c 相似（距離=2），所以 a、b、c 應在同一組
        let hashes = vec![
            vec![0b11111111],
            vec![0b11111100], // 距離 a=2
            vec![0b11110000], // 距離 b=2, 距離 a=4
        ];
        let groups = find_similar_groups(hashes, 4).unwrap();
        assert_eq!(groups.len(), 1);
        let (members, _) = &groups[0];
        assert_eq!(members.len(), 3);
    }

    #[test]
    fn test_union_find_path_compression() {
        let mut parent: Vec<usize> = vec![0, 0, 1, 2, 3];
        // 4 -> 3 -> 2 -> 1 -> 0
        let root = find(&mut parent, 4);
        assert_eq!(root, 0);
        // 路徑壓縮後，4 應直接指向 0
        assert_eq!(parent[4], 0);
    }
}
