//! 瀏覽器狀態機、非破壞式編輯 Session、In-Memory Undo/Redo 與視口狀態

pub mod history;
pub mod session;
pub mod viewport;

pub use history::{HistoryManager, HistoryNode, DEFAULT_MAX_HISTORY_STEPS};
pub use session::EditingSession;
pub use viewport::ViewportState;

/// 取得 viewer-state 模組版本
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}