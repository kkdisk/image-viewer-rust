//! 瀏覽器狀態機、非破壞式編輯 Session 與 In-Memory Undo/Redo

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}