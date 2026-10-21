# 實作計畫 (Implementation Plan): 純 Rust 圖片瀏覽與編輯工具

**關聯規格**：[docs/SPEC_RUST_IMAGE_VIEWER.md](docs/SPEC_RUST_IMAGE_VIEWER.md)  
**狀態**：進行中 (Phase 1 執行中)  

---

## 1. 系統架構與相依關係 (Dependency Graph)

```
[core-image] (影像編解碼、Rayon微調、原子存檔) ──┐
[core-archive] (7z/zip安全解壓、資源回收桶刪除)  ───┼──→ [viewer-state] (狀態機、Session、Undo/Redo)
                                                   │          │
[duplicate-finder] (MD5、pHash、Union-Find) ───────┘          ▼
                                                       [app-ui] (egui/eframe 渲染與互動面板)
```

**架構決策原則 (Architecture Decisions)**：
1. **零磁碟 I/O 快照**：廢除舊版寫入磁碟 PNG 暫存檔的 Undo 機制，改以記憶體 `Arc<RgbaImage>` 與 Command 結構維護歷史，最高保留 20 步，大幅降低耗電與硬碟磨損。
2. **多執行緒與 GPU 渲染分離**：主執行緒透過 `egui` 在 GPU 上即時繪製視口紋理；所有的圖片載入、縮圖產生、微調平行運算皆在背景執行緒池（`tokio` + `rayon`）處理，完成後透過 Channel 交付 GPU 紋理更新，絕不阻塞 UI。
3. **垂直切片交付**：依照能力地圖逐層遞進，每個階段都有可獨立編譯與測試的驗證成果。

---

## 2. 階段劃分與檢查點 (Phases & Checkpoints)

### Phase 1: 核心計算與狀態底座 (Foundation)
* 建立 Cargo Workspace。
* 實作 `core-image`（載入、EXIF 方向、Rayon 白平衡/微調、原子存檔）。
* 實作 `viewer-state`（文件狀態機、非破壞式編輯 Session、In-Memory Undo/Redo）。
* **Checkpoint 1**：所有單元測試通過，模擬無 UI 下的「載入 → 旋轉 → 滑桿微調 → 套用 → Undo → 原子寫檔」端到端測試通過。

### Phase 2: MVP 基本瀏覽器 (Core Viewer)
* 實作 `app-ui` 基礎骨架：視窗建立、egui 主題與字型設置。
* 實作可縮放與拖曳平移的 GPU 影像畫布（Texture Viewport）。
* 實作檔案列表導覽（上一張/下一張、快捷鍵）。
* 實作底部 Filmstrip 縮圖列（背景非同步載入）。
* **Checkpoint 2**：[x] 通過。能以單一二進位檔正常啟動，瀏覽任意目錄之圖片，平移縮放流暢，縮圖列渲染正常。

### Phase 3: 非破壞式微調與雙圖比較 (Editing & Comparison)
* 實作側邊微調面板（五個滑桿：色溫、色調、亮度、對比、飽和度）。
* 實作即時預覽防抖與 Session 管理（取消回滾、套用提交歷史、重設）。
* 實作 Undo (Ctrl+Z) / Redo (Ctrl+Y)。
* 實作左右分割的雙圖對照視圖（原圖對照 / 外部參考圖），視口操作雙向同步。
* **Checkpoint 3**：能進行即時微調、取消/套用、復原/重做，並可在雙圖模式下同步平移縮放。

### Phase 4: 重複圖片檢查器與壓縮包支援 (Duplicate Finder & Archive)
* 實作 `duplicate-finder`（MD5、pHash、Union-Find 分群演算法與單元測試）。
* 實作查重工具視窗/面板：批次掃描進度條、相似群組列表、雙圖視覺差異高亮、安全移至資源回收桶。
* 實作 `core-archive`（7z / zip 安全唯讀解壓預覽，Path Traversal 防護）。
* **Checkpoint 4**：能秒級完成千張圖片查重，且能直接拖放開啟 7z 壓縮檔預覽圖片。

### Phase 5: 輔助工具與發布打包 (Polish & Release)
* 實作直方圖（RGB / 亮度）即時繪製元件。
* 實作滑鼠游標局部放大鏡。
* 進行 Windows Release 體積最佳化（LTO、strip 等編譯參數）。
* **Final Checkpoint**：產出單一獨立可執行檔，執行完整迴歸測試與記憶體洩漏檢測。

---

## 3. 風險評估與緩解策略 (Risks & Mitigations)

| 風險項目 | 影響度 | 緩解策略 |
| :--- | :---: | :--- |
| **超大解析度圖片紋理超過 GPU 限制** | 高 | 在 `core-image` 中設定 `MAX_SAFE_PIXELS = 64M`；若尺寸超過 GPU 最大紋理大小，自動下採樣為預覽紋理，存檔時仍以完整解析度輸出。 |
| **滑桿頻繁拖曳造成 CPU 運算堆積** | 中 | 採用防抖（Debounce / Latest-Only）佇列，背景運算若發現有更新的參數請求，立即丟棄舊計算任務。 |
| **Windows 檔案路徑編碼與長路徑問題** | 中 | 全面使用 `std::path::PathBuf` 與 UNC 長路徑前綴規範，徹底杜絕編碼崩潰。 |
