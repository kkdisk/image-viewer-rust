//! Union-Find 並查集分群與 Hamming 距離相似度聚類演算法

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 重複或相似圖片群組
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    /// 群組內重複/相似之檔案路徑清單
    pub items: Vec<PathBuf>,
    /// 該群組內成對圖片的最大 Hamming 距離 (0 代表完全一致)
    pub max_distance: u32,
}

/// 計算兩個 byte 陣列之 Hamming distance（不同 bit 數）
#[inline]
pub fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    // 快速路徑：若為標準 8 bytes (64-bit pHash)，直接使用單一 CPU POPCNT 指令
    if a.len() == 8 && b.len() == 8 {
        let mut arr_a = [0u8; 8];
        let mut arr_b = [0u8; 8];
        arr_a.copy_from_slice(a);
        arr_b.copy_from_slice(b);
        let val_a = u64::from_ne_bytes(arr_a);
        let val_b = u64::from_ne_bytes(arr_b);
        return (val_a ^ val_b).count_ones();
    }

    let min_len = a.len().min(b.len());
    let max_len = a.len().max(b.len());
    let mut dist = 0u32;

    for i in 0..min_len {
        dist += (a[i] ^ b[i]).count_ones();
    }

    // 長度不足的 byte 視為全數相異
    dist += (max_len - min_len) as u32 * 8;
    dist
}

/// 具備路徑壓縮 (Path Compression) 與按秩合併 (Union by Rank) 的並查集
#[derive(Debug)]
pub struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u32>,
    max_dist: Vec<u32>,
}

impl UnionFind {
    /// 初始化指定大小之並查集
    pub fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
            max_dist: vec![0; size],
        }
    }

    /// 尋找節點根節點 (遞迴路徑壓縮)
    pub fn find(&mut self, i: usize) -> usize {
        let mut root = i;
        while root != self.parent[root] {
            root = self.parent[root];
        }

        // 路徑壓縮
        let mut curr = i;
        while curr != root {
            let next = self.parent[curr];
            self.parent[curr] = root;
            curr = next;
        }

        root
    }

    /// 合併兩個集合並記錄最大距離
    pub fn union(&mut self, i: usize, j: usize, distance: u32) {
        let root_i = self.find(i);
        let root_j = self.find(j);

        if root_i == root_j {
            if distance > self.max_dist[root_i] {
                self.max_dist[root_i] = distance;
            }
            return;
        }

        let combined_max = self.max_dist[root_i].max(self.max_dist[root_j]).max(distance);

        // 按秩合併
        match self.rank[root_i].cmp(&self.rank[root_j]) {
            std::cmp::Ordering::Less => {
                self.parent[root_i] = root_j;
                self.max_dist[root_j] = combined_max;
            }
            std::cmp::Ordering::Greater => {
                self.parent[root_j] = root_i;
                self.max_dist[root_i] = combined_max;
            }
            std::cmp::Ordering::Equal => {
                self.parent[root_j] = root_i;
                self.rank[root_i] += 1;
                self.max_dist[root_i] = combined_max;
            }
        }
    }

    /// 取得指定根節點之最大距離
    pub fn max_distance(&self, root: usize) -> u32 {
        self.max_dist[root]
    }
}

/// 依據 pHash 與 Hamming 距離門檻進行相似圖片分群
///
/// 僅返回成員數 >= 2 之群組，並依據群組成員數量降序排列。
pub fn find_similar_groups<P: AsRef<Path>>(
    hashes: &[(P, Vec<u8>)],
    threshold: u32,
) -> Vec<DuplicateGroup> {
    let n = hashes.len();
    if n < 2 {
        return Vec::new();
    }

    let mut uf = UnionFind::new(n);

    // 預先提取 64-bit 整數快取，使 O(N^2) 比較以純 CPU POPCNT 指令飛速執行
    let fast_hashes: Vec<Option<u64>> = hashes
        .iter()
        .map(|(_, h)| {
            if h.len() == 8 {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(h);
                Some(u64::from_ne_bytes(arr))
            } else {
                None
            }
        })
        .collect();

    // O(N^2) 配對比對
    for i in 0..n {
        if hashes[i].1.is_empty() {
            continue;
        }

        let fast_i = fast_hashes[i];
        let hash_i = &hashes[i].1;

        for j in (i + 1)..n {
            if hashes[j].1.is_empty() {
                continue;
            }

            let dist = if let (Some(v_a), Some(v_b)) = (fast_i, fast_hashes[j]) {
                (v_a ^ v_b).count_ones()
            } else {
                hamming_distance(hash_i, &hashes[j].1)
            };

            if dist <= threshold {
                uf.union(i, j, dist);
            }
        }
    }

    // 依根節點分組收集
    let mut group_map: HashMap<usize, Vec<PathBuf>> = HashMap::new();
    for (i, (path, _)) in hashes.iter().enumerate() {
        let root = uf.find(i);
        group_map.entry(root).or_default().push(path.as_ref().to_path_buf());
    }

    let mut results: Vec<DuplicateGroup> = group_map
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(root, members)| DuplicateGroup {
            items: members,
            max_distance: uf.max_distance(root),
        })
        .collect();

    // 依群組大小降序排列
    results.sort_by_key(|b| std::cmp::Reverse(b.items.len()));
    results
}

/// 依據 MD5 檔案雜湊進行精確重複分群 (完全相同檔案)
pub fn find_exact_md5_groups<P: AsRef<Path>>(
    md5_list: &[(P, String)],
) -> Vec<DuplicateGroup> {
    let mut map: HashMap<&str, Vec<PathBuf>> = HashMap::new();

    for (path, hash) in md5_list {
        if !hash.is_empty() && !hash.starts_with("ERROR") {
            map.entry(hash.as_str())
                .or_default()
                .push(path.as_ref().to_path_buf());
        }
    }

    let mut results: Vec<DuplicateGroup> = map
        .into_iter()
        .filter(|(_, members)| members.len() >= 2)
        .map(|(_, members)| DuplicateGroup {
            items: members,
            max_distance: 0,
        })
        .collect();

    results.sort_by_key(|b| std::cmp::Reverse(b.items.len()));
    results
}
