//! 保留 Unicode 路徑的啟動參數解析，不在介面執行緒掃描資料夾。
use std::{ffi::OsString, path::PathBuf};
#[derive(Default, Debug, PartialEq, Eq)]
pub struct StartupOptions {
    pub portable: bool,
    pub path: Option<PathBuf>,
}
pub fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<StartupOptions, String> {
    let mut options = StartupOptions::default();
    let mut positional = false;
    for arg in args {
        if !positional && arg == "--" {
            positional = true;
            continue;
        }
        if !positional && arg == "--portable" {
            options.portable = true;
            continue;
        }
        if !positional && arg.to_str().is_some_and(|arg| arg.starts_with('-')) {
            return Err("不支援此啟動選項；路徑以 - 開頭時請先加 --".into());
        }
        if options.path.replace(PathBuf::from(arg)).is_some() {
            return Err("一次只能指定一個圖片、資料夾或壓縮檔".into());
        }
    }
    if let Some(path) = &mut options.path
        && path.is_relative()
    {
        *path = std::env::current_dir()
            .map_err(|error| error.to_string())?
            .join(&*path);
    }
    Ok(options)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_and_portable_are_independent() {
        let parsed = parse_args(["--portable", "相片 資料夾/圖2.png"].map(OsString::from)).unwrap();
        assert!(parsed.portable);
        assert!(parsed.path.unwrap().ends_with("相片 資料夾/圖2.png"));
        assert!(
            !parse_args(["--", "--portable"].map(OsString::from))
                .unwrap()
                .portable
        );
        assert!(parse_args(["a.png", "b.png"].map(OsString::from)).is_err());
    }
}
