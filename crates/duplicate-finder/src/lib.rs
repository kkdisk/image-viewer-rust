//! 重複圖片檢測模組 (MD5, pHash, Union-Find)

pub mod error;
pub mod hashing;
pub mod union_find;

pub use error::DuplicateError;
pub use hashing::{
    DEFAULT_HASH_SIZE, batch_compute_hashes, compute_md5, compute_phash, compute_phash_from_image,
};
pub use union_find::{
    DuplicateGroup, UnionFind, find_exact_md5_groups, find_similar_groups, hamming_distance,
};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
