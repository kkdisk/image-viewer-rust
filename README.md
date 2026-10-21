# Rust Image Viewer (純 Rust 輕量高效圖片瀏覽與編輯工具)

## 簡介
本專案為使用純 Rust (\gui\ + \frame\ + \ayon\) 開發的高效能桌面圖片瀏覽、非破壞式編輯與重複圖片管理工具。

## 特色
- **極致輕量**：單一原生二進位檔（< 15MB），冷啟動時間 < 50ms。
- **高幀率畫布**：GPU 硬體加速視口，支援超滑順滾輪縮放與拖曳平移。
- **非破壞式即時微調**：白平衡（色溫/色調）、微調（亮度/對比/飽和度）即時預覽，支援完整 Undo/Redo（記憶體指令，零磁碟 I/O 磨損）與原子儲存。
- **雙圖同步比對**：原圖對照或外部唯讀參考圖，雙向視口同步。
- **重複圖片檢查器**：多執行緒 pHash + MD5 批次計算，Union-Find 聚類分群，支援移至資源回收桶。
- **壓縮檔支援**：直接唯讀預覽 7z / zip 壓縮包內圖片。

## 專案結構 (Cargo Workspace)
- \crates/core-image\：影像解碼、EXIF 校正、Rayon 平行微調管線、原子存檔
- \crates/core-archive\：壓縮檔安全預覽、資源回收桶整合
- \crates/duplicate-finder\：pHash、MD5、Union-Find 分群
- \crates/viewer-state\：非破壞式 Session、In-Memory Undo/Redo
- \crates/app-ui\：桌面應用進入點與 egui 畫布元件
- \ef_algorithms/\：自舊專案移植之演算法參考資產

## 開發與進度
詳細規格請參閱 [docs/SPEC_RUST_IMAGE_VIEWER.md](docs/SPEC_RUST_IMAGE_VIEWER.md)。
開發進度與任務追蹤請參閱 [tasks/todo.md](tasks/todo.md)。