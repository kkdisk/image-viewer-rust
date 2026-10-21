# 軟體規格說明書 (Spec): 純 Rust 圖片瀏覽與編輯工具

**文件識別碼**：SPEC-RUST-IMAGE-VIEWER  
**版本**：1.0.0  
**日期**：2026-10-21  
**關聯意圖**：[docs/intent/rust_image_viewer_migration.md](docs/intent/rust_image_viewer_migration.md)

---

## 1. 模組能力地圖 (Capability Map)

本系統拆分為五個獨立且相依性明確的核心模組：

| 模組 ID (`module_id`) | 模組責任 | 相依模組 |
| :--- | :--- | :--- |
| `core-image` | 影像編解碼、EXIF 解析、白平衡/微調平行管線、幾何轉換 (旋轉/翻轉)、原子存檔 | 無 (底層基礎) |
| `core-archive` | 壓縮檔（7z/zip）安全預覽與解壓、Zip-bomb 防範、安全檔案刪除（資源回收桶） | 無 |
| `duplicate-finder` | MD5 檔案雜湊、pHash 感知雜湊、Union-Find 分群、相似度比對 | `core-image` |
| `viewer-state` | 文件狀態機、非破壞式編輯 Session、In-Memory Undo/Redo 歷史、雙圖視口同步邏輯 | `core-image` |
| `app-ui` | `egui` + `eframe` 桌面視窗、GPU 畫布渲染、Filmstrip 縮圖列、直方圖與互動面板 | `viewer-state`, `core-archive`, `duplicate-finder` |

**建置順序 (Build Order)**：  
`core-image` & `core-archive` → `duplicate-finder` & `viewer-state` → `app-ui`

---

## 2. 目標 (Objective)

打造單一原生二進位檔（< 15MB）的現代桌面看圖與編輯軟體。
* **主要場景**：超快冷啟動檢視高解析度圖片、平滑滾輪縮放、非破壞式滑桿即時微調、雙圖對照（原圖/目前結果或外部參考圖）、大量圖片批次感知查重與壓縮包直接預覽。
* **使用者體驗原則**：介面清爽俐落、無延遲反饋、操作絕不卡頓、存檔保證原子安全不損壞原檔。

---

## 3. 技術棧 (Tech Stack)

* **程式語言**：Rust (Edition 2024, MSRV 1.85+)
* **GUI 框架**：`egui` (0.29+) + `eframe` (支援 WGPU 或 Glow 硬體加速渲染)
* **影像處理**：
  * `image` crate (0.25+)：支援 JPEG, PNG, BMP, GIF, TIFF, WebP 編解碼
  * `kamadak-exif`：EXIF 元資料解析與自動 Orientation 校正
* **並行與非同步運算**：
  * `rayon` (1.10+)：CPU 像素平行運算、批次雜湊與矩陣運算
  * `tokio` (1.40+)：背景非同步檔案掃描、縮圖產生與 I/O 監聽
* **檔案與安全處理**：
  * `trash`：跨平台安全刪除至資源回收桶
  * `sevenz-rust` & `zip`：安全解壓縮與目錄掃描
  * `tempfile`：原子寫檔暫存
* **演算法**：
  * `md-5`：精確檔案校驗
  * DCT + pHash 演算法（延續原 `rust_ext` 優良實作）

---

## 4. 執行與建置指令 (Commands)

* **開發測試運行**：
  ```bash
  cargo run --bin image_viewer
  ```
* **發布編譯 (優化體積與效能)**：
  ```bash
  cargo build --release
  ```
* **單元與整合測試**：
  ```bash
  cargo test --all-targets --all-features
  ```
* **程式碼檢查與格式化**：
  ```bash
  cargo clippy --all-targets -- -D warnings
  cargo fmt --check
  ```

---

## 5. 專案目錄結構 (Project Structure)

本專案採 Cargo Workspace 結構組織：

```
image-viewer-rs/
├── Cargo.toml                  # Workspace 根配置
├── crates/
│   ├── core-image/             # 底層影像處理、像素管線、幾何變換與原子存檔
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── ops.rs          # 白平衡、微調、濾鏡 (Rayon 平行)
│   │       ├── exif.rs         # EXIF 方向與元資料
│   │       └── io.rs           # 限制像素載入與原子儲存
│   ├── core-archive/           # 壓縮檔安全讀取
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── extractor.rs    # Path Traversal & Zip Bomb 驗證
│   ├── duplicate-finder/       # 重複圖片檢查器核心
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── hashing.rs      # MD5 & pHash (DCT)
│   │       └── union_find.rs   # 分群與 Hamming 距離
│   ├── viewer-state/           # 應用狀態與歷史管理
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── history.rs      # In-Memory Command & Snapshot Undo/Redo
│   │       ├── session.rs      # 非破壞式微調 Session
│   │       └── viewport.rs     # 縮放、平移與雙圖同步
│   └── app-ui/                 # egui 桌面主程式
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs         # 應用進入點
│           ├── app.rs          # eframe::App 實作
│           ├── canvas/         # 圖片畫布與雙圖分割視圖
│           ├── panels/         # 側邊調整欄、Filmstrip、直方圖
│           └── dialogs/        # 查重面板、設定與檔案對話框
└── tests/                      # 跨模組端到端整合測試
```

---

## 6. 程式碼規範 (Code Style)

1. **命名原則**：
   * 型別、結構體、列舉：`PascalCase`（如 `AdjustmentSession`, `ImageBuffer`）
   * 函式、變數、模組：`snake_case`（如 `apply_white_balance`, `current_scale`）
   * 常數：`SCREAMING_SNAKE_CASE`（如 `MAX_SAFE_PIXELS`）
2. **錯誤處理 (Error Handling)**：
   * 庫模組一律使用 `thiserror` 定義強型別錯誤列舉，杜絕裸字串錯誤。
   * 應用層（UI/進入點）使用 `anyhow` 處理非預期錯誤。
   * 禁止在非測試代碼中使用 `unwrap()` 或 `expect()`。
3. **不可變性與記憶體安全**：
   * 圖片資料以 `Arc<RgbImage>` 或 `Arc<RgbaImage>` 共享，非破壞式編輯時只在需要變更時 Copy-on-write。
   * 範例樣式：
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq)]
   pub struct AdjustmentParams {
       pub temperature: i32, // -100 ~ 100
       pub tint: i32,        // -100 ~ 100
       pub brightness: f32,  // 0.0 ~ 2.0 (1.0 = 不變)
       pub contrast: f32,    // 0.0 ~ 2.0 (1.0 = 不變)
       pub saturation: f32,  // 0.0 ~ 2.0 (1.0 = 不變)
   }
   ```

---

## 7. 測試策略 (Testing Strategy)

* **單元測試 (Unit Tests)**：
  * `core-image`：驗證白平衡、微調演算法計算精準度（浮點數差異 < 1e-4），測試 EXIF 方向 1~8 的旋轉正確性。
  * `duplicate-finder`：驗證同一張圖在旋轉或不同壓縮下 pHash 的 Hamming distance，測試 Union-Find 路徑壓縮。
  * `viewer-state`：驗證 Undo/Redo 歷史上限、分支清除、未儲存標記同步。
* **邊界測試 (Edge Cases)**：
  * 超大圖（超過 64M 像素）記憶體防護拒絕測試。
  * 損毀圖片檔案、非安全壓縮檔（包含 `../` 路徑）拒絕測試。
* **整合測試 (Integration Tests)**：
  * 模擬載入 → 調整參數 → 套用歷史 → Undo → 原子寫入暫存檔的完整流程。

---

## 8. 行為邊界 (Boundaries)

* **永遠遵守 (Always)**：
  * 圖片輸出寫檔必須遵循「先寫入同目錄暫存檔，編碼成功後再替換目標檔」的原子儲存規則。
  * 載入前必須先檢查圖片長寬展開後的總像素數，防止惡意解壓炸彈。
  * 所有的繁重運算（大圖解碼、批次 pHash、重縮圖）必須在非同步/執行緒池執行，UI 執行緒只負責渲染。
* **必須先確認 (Ask First)**：
  * 引入任何含有 C/C++ 外部動態鏈結依賴的 Crate（嚴格防範破壞單一純 Rust 靜態二進位目標）。
  * 變更已儲存檔案的色彩空間預設設定。
* **嚴禁行為 (Never)**：
  * 嚴禁在未經使用者點擊「套用」前，將微調滑桿預覽覆寫到目前編輯中的基礎影像。
  * 嚴禁直接覆寫壓縮檔內的檔案（壓縮檔圖片一律強制要求另存新檔）。
  * 嚴禁在主 UI 執行緒執行阻塞性磁碟 I/O。

---

## 9. 驗收完成標準 (Success Criteria)

1. [ ] **建置輸出**：`cargo build --release` 成功產出單一可執行檔，Windows 上無額外 DLL 依賴。
2. [ ] **效能指標**：冷啟動 < 50ms；4K 圖片滾輪連續縮放與平移維持 60 FPS 以上。
3. [ ] **非破壞式編輯**：五個微調滑桿即時預覽，取消立即恢復，套用記錄單一 Undo 節點，支援完整 Redo。
4. [ ] **雙圖比較**：支援原圖比對與外部參考圖比對，同步縮放/平移無延遲且無事件回授循環。
5. [ ] **查重工具**：1,000 張圖片的批次 MD5 + pHash 計算與分群能在 1 秒內完成（多核平行）。
6. [ ] **安全性**：通過路徑穿透、Zip-bomb 與原子寫檔測試。
