//! 僅允許回收的刪除入口；作業系統預計永久刪除時，在動作前拒絕。
use std::path::Path;
#[derive(Debug, thiserror::Error)]
pub enum RecycleError {
    #[error("回收失敗：{0}")]
    Operation(String),
    #[error("此平台尚未驗證回收保證，已拒絕刪除")]
    Unsupported,
}
pub fn recycle_file(path: &Path) -> Result<(), RecycleError> {
    #[cfg(windows)]
    {
        windows_recycle::recycle(path).map_err(|error| RecycleError::Operation(error.to_string()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(RecycleError::Unsupported)
    }
}
#[cfg(windows)]
mod windows_recycle {
    use super::*;
    use std::{
        os::windows::ffi::OsStrExt,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicI32, Ordering},
        },
    };
    use windows::{
        Win32::{Foundation::E_ABORT, System::Com::*, UI::Shell::*},
        core::{HRESULT, PCWSTR, Ref, implement},
    };
    fn check_recycle(flags: u32) -> windows::core::Result<()> {
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32 == 0 {
            Err(E_ABORT.into())
        } else {
            Ok(())
        }
    }
    #[implement(IFileOperationProgressSink)]
    struct RecycleSink {
        checked: Arc<AtomicBool>,
        result: Arc<AtomicI32>,
    }
    #[allow(non_snake_case)]
    impl IFileOperationProgressSink_Impl for RecycleSink_Impl {
        fn StartOperations(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn FinishOperations(&self, _: windows::core::HRESULT) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreRenameItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostRenameItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
            _: windows::core::HRESULT,
            _: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreMoveItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostMoveItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
            _: windows::core::HRESULT,
            _: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreCopyItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostCopyItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
            _: windows::core::HRESULT,
            _: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PreDeleteItem(&self, flags: u32, _: Ref<IShellItem>) -> windows::core::Result<()> {
            check_recycle(flags)?;
            self.checked.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn PostDeleteItem(
            &self,
            _: u32,
            _: Ref<IShellItem>,
            result: HRESULT,
            _: Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            self.result.store(result.0, Ordering::SeqCst);
            result.ok()
        }
        fn PreNewItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn PostNewItem(
            &self,
            _: u32,
            _: windows::core::Ref<IShellItem>,
            _: &windows::core::PCWSTR,
            _: &windows::core::PCWSTR,
            _: u32,
            _: windows::core::HRESULT,
            _: windows::core::Ref<IShellItem>,
        ) -> windows::core::Result<()> {
            Ok(())
        }
        fn UpdateProgress(&self, _: u32, _: u32) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResetTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn PauseTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
        fn ResumeTimer(&self) -> windows::core::Result<()> {
            Ok(())
        }
    }
    pub fn recycle(path: &Path) -> windows::core::Result<()> {
        if !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file()) {
            return Err(E_ABORT.into());
        }
        let path = std::path::absolute(path).map_err(|_| windows::core::Error::from(E_ABORT))?;
        let wide = shell_path(&path);
        if wide[..wide.len() - 1].contains(&0) {
            return Err(E_ABORT.into());
        }
        // 背景執行緒初始化 COM；介面均在結束 COM 前釋放，UTF-16 緩衝在呼叫期間存活。
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        struct ComGuard;
        impl Drop for ComGuard {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _guard = ComGuard;
        let checked = Arc::new(AtomicBool::new(false));
        let result = Arc::new(AtomicI32::new(E_ABORT.0));
        let sink: IFileOperationProgressSink = RecycleSink {
            checked: checked.clone(),
            result: result.clone(),
        }
        .into();
        // 僅排程單一項目，回呼拒絕任何不能回收的動作；不建立永久刪除備援。
        unsafe {
            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            operation.SetOperationFlags(
                FOF_NO_UI | FOF_WANTNUKEWARNING | FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE,
            )?;
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
            operation.DeleteItem(&item, &sink)?;
            operation.PerformOperations()?;
            if operation.GetAnyOperationsAborted()?.as_bool() || !checked.load(Ordering::SeqCst) {
                return Err(E_ABORT.into());
            }
        }
        HRESULT(result.load(Ordering::SeqCst)).ok()
    }
    fn shell_path(path: &Path) -> Vec<u16> {
        let mut units: Vec<u16> = path.as_os_str().encode_wide().collect();
        if units.starts_with(&[92, 92, 63, 92]) {
            units.drain(..4);
            if units.starts_with(&[85, 78, 67, 92]) {
                units.splice(..4, [92, 92]);
            }
        }
        units.push(0);
        units
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn shell_paths_preserve_unicode_and_remove_verbatim_prefix() {
            assert_eq!(
                shell_path(Path::new(r"\\?\C:\圖片.png")),
                shell_path(Path::new(r"C:\圖片.png"))
            );
            assert_eq!(
                shell_path(Path::new(r"\\?\UNC\主機\分享\圖片.png")),
                shell_path(Path::new(r"\\主機\分享\圖片.png"))
            );
        }
        #[test]
        fn permanent_delete_flags_are_rejected_before_io() {
            assert!(check_recycle(0).is_err());
            assert!(check_recycle(TSF_DELETE_RECYCLE_IF_POSSIBLE.0 as u32).is_ok());
        }
    }
}
