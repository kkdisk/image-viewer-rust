//! 最近清單：保留成功開啟的實際路徑、排序時間與 Windows 序數路徑識別。
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: PathBuf,
    pub opened_ms: u64,
}

fn normalized(path: &Path) -> PathBuf {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = if let Some(rest) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else {
        text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned()
    };
    PathBuf::from(text.trim_end_matches('\\'))
}

pub fn same_path(a: &Path, b: &Path) -> bool {
    let a = normalized(a);
    let b = normalized(b);
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn CompareStringOrdinal(
                a: *const u16,
                a_len: i32,
                b: *const u16,
                b_len: i32,
                ignore_case: i32,
            ) -> i32;
        }
        let a: Vec<u16> = a.as_os_str().encode_wide().collect();
        let b: Vec<u16> = b.as_os_str().encode_wide().collect();
        let (Ok(a_len), Ok(b_len)) = (i32::try_from(a.len()), i32::try_from(b.len())) else {
            return false;
        };
        // 緩衝區在呼叫期間存活，長度已檢查；僅做 Windows 序數比較，不讀取磁碟。
        unsafe { CompareStringOrdinal(a.as_ptr(), a_len, b.as_ptr(), b_len, 1) == 2 }
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

pub fn merge(a: &[RecentEntry], b: &[RecentEntry]) -> Vec<RecentEntry> {
    let mut entries: Vec<_> = a.iter().chain(b).cloned().collect();
    entries.sort_by(|a, b| {
        b.opened_ms
            .cmp(&a.opened_ms)
            .then_with(|| a.path.cmp(&b.path))
    });
    let mut result: Vec<RecentEntry> = Vec::new();
    for entry in entries {
        if !result.iter().any(|kept| same_path(&kept.path, &entry.path)) {
            result.push(entry);
        }
        if result.len() == 20 {
            break;
        }
    }
    result
}

pub fn timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis().min(u128::from(u64::MAX)) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recent_merge_is_bounded_and_keeps_newest_identity() {
        let entries: Vec<_> = (0..25)
            .map(|index| RecentEntry {
                path: PathBuf::from(format!("C:/Pictures/{index}.png")),
                opened_ms: index,
            })
            .collect();
        let newest = RecentEntry {
            path: PathBuf::from("C:/Pictures/24.png"),
            opened_ms: 100,
        };
        let merged = merge(&entries, &[newest]);
        assert_eq!(merged.len(), 20);
        assert_eq!(merged[0].opened_ms, 100);
        #[cfg(windows)]
        assert!(same_path(
            Path::new("C:/FOTOS/Ä/圖.PNG"),
            Path::new("\\\\?\\c:\\fotos\\ä\\圖.png")
        ));
    }
}
