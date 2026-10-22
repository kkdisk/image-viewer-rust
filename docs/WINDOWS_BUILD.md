# Windows 建置與驗收

## 本機驗證

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo build --release -p app-ui --locked --offline
./scripts/package_windows.ps1
```

首次下載相依套件時移除 `--offline`。`Cargo.lock` 固定應用程式相依版本，應納入版本控制。工作目錄內由 `rust-toolchain.toml` 選用 Rust 1.99.1，五個套件的最低版本與 CI 同步固定為 1.99.1。

本機 Build Tools 若安裝於 `C:\BuildTools`，可在 PowerShell 使用下列方式載入 C++／SDK 環境後執行 Cargo：

```powershell
cmd.exe /d /c 'call C:\BuildTools\Common7\Tools\VsDevCmd.bat -arch=x64 -host_arch=x64 >nul && cargo test --workspace --locked --offline'
```

封裝腳本以 Cargo 中的應用程式版本命名 ZIP，內含 `app-ui.exe`，並另外產生 ZIP 的 SHA-256 檢查碼。執行檔必須小於 15 MB（15,000,000 位元組）；產物放在已忽略的 `dist/`。顯示 MiB 數值時另以 1,048,576 換算，不改變門檻。

### v0.3.1 開發基線（2026-10-22）

- 起初本機只有 Rust 1.96.0 且缺少 `link.exe`。使用者授權「安裝」後，已安裝 Microsoft Visual Studio Build Tools 至 `C:\BuildTools`（安裝退出碼 0）、MSVC 14.44.35207 與 Windows SDK 10.0.26100.0，編譯環境已恢復。
- 原訂 1.85 與鎖定相依不相容：image 要求 1.88、uuid 要求 1.89。使用者指定改為 1.99.1；已安裝並核對 `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`，主機為 `x86_64-pc-windows-msvc`，不再宣稱支援 1.85。
- GitHub API 已核對 v0.3.0 [標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39760086200) 及 [main CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39760086220) 均為 `completed/success`，兩者提交皆為 `7d2e7324d43466993cdbb07aa46533acfa5d2f7c`。
- [v0.3.0 Release](https://github.com/kkdisk/image-viewer-rust/releases/tag/v0.3.0) 已公開（`draft: false`）。本輪已下載遠端 ZIP 與檢查碼檔：ZIP 為 3,283,429 位元組，SHA-256 為 `786cc4fbd715420c52f1397ed8c86bb9993734ad8fae4f6ae7f949652d3b9b66`，與 API 摘要及 `.sha256` 檔一致。ZIP 僅含 `app-ui.exe`，解壓長度 6,580,736 位元組，已確認包含完整的 `版本 v0.3.0 (Rust 2024 Edition)` 文字，未執行下載內容。不得與先前本機 3,302,207 位元組的 ZIP 混用。

## 自動流程

v0.3.1 起使用 Windows runner 與固定 Rust 1.99.1；下段原有「穩定版」指該固定工具鏈，不再隨 stable 更新。

[Windows 流程](../.github/workflows/windows.yml) 在拉取請求、推送至 `main`、推送 `v*` 標籤及手動觸發時執行格式、Clippy、工作區測試、發布建置及封裝。使用 Windows runner 與穩定版 Rust；一般工作只有儲存庫讀取權限。建置產物保留 14 天，可依 [GitHub 官方說明](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts) 下載。

標籤推送額外核對標籤必須等於應用程式版本的 `v` 前綴形式，例如套件 `0.1.0` 對應 `v0.1.0`。品質工作成功後，獨立的發布工作取得本次產物，以 `contents: write` 建立附有 ZIP 與檢查碼的 GitHub Release 草稿。草稿須由維護者確認桌面驗收結果後發布。重跑既有標籤不覆寫既有 Release；已存在時會回報失敗，應先查明原草稿及產物狀態。

流程的 checkout 與產物上傳介面依 [checkout 官方文件](https://github.com/actions/checkout) 與 [upload-artifact 官方文件](https://github.com/actions/upload-artifact) 設定。本機測試不能視為 GitHub runner 實跑通過；推送後應另行查閱對應提交與標籤的工作結果。

## 桌面驗收

### v0.3.1 本機交付（2026-10-22）

- Rust 1.99.1 全工作區回歸 91 項通過、1 項效能量測忽略；另新增動畫警告實際指標點擊回歸 1 項通過，共 92 項。最後修改後已重跑動畫儲存測試及全部目標 Clippy，零警告通過；格式、差異及封裝腳本語法檢查通過。
- `cargo build --release -p app-ui --locked --offline` 成功。執行檔 `target/release/app-ui.exe` 為 6,653,440 位元組（約 6.35 MiB），含完整 `版本 v0.3.1 (Rust 2024 Edition)` 文字；五個套件版本皆為 0.3.1，最低 Rust 皆為 1.99.1。
- 封裝 `dist/rust-image-viewer-0.3.1-windows-x64.zip` 為 3,236,526 位元組，僅含本次執行檔，已核對 ZIP 內執行檔與建置檔 SHA-256 相同。ZIP SHA-256 為 `9d19d70ced84144c39f9253bd9e08a420bdfbf1f775a5b1bfbdc50864b10fd06`，與附檔一致。
- 封裝腳本儲存為帶 BOM 的 UTF-8，確保 Windows PowerShell 5.1 可正確解析中文訊息，已直接執行封裝通過。
- 本輪尚未執行原生桌面驗收，未提交、推送或建立 v0.3.1 標籤；本版遠端 CI 為 **Unknown（尚未觸發）**。下列桌面項目及第4批新增案例仍需驗收。

第4批待驗收：已存底圖預覽完成與取消／套用後狀態、查重大圖快速切換及關閉重開、含損毀圖的大目錄完成交付、真實 GPU 長條圖與雙圖及完整尺寸另存、動畫 GIF／WebP 覆寫／另存／儲存後離開的警告與取消，以及壓縮來源動畫只能另存外部。

### v0.3.1 提交前重驗

使用者另行授權提交、推送與標記。重驗時 15 項 app-ui 單元測試通過，`about_version_test` 執行檔被本機 Windows 應用程式控制原則阻擋（os error 4551）；一般執行與工具核准重試皆相同，測試未執行，不計為通過。本次完整工作區結果需以推送後對應提交的 Windows CI 核對。Clippy、格式重驗通過；先前 92 項結果為歷次紀錄。發布執行檔與原 ZIP 不一致，已安排重新建置及封裝，以下新紀錄優先於上節舊產物數值。

本輪重建成功：執行檔 6,653,440 位元組，包含完整 v0.3.1 版本文字；重新封裝 ZIP 為 3,236,526 位元組，SHA-256 為 `fd449b3779a9730194cb21ab6b4e87f70b524948edd864a87827293e8b7a7e2e`。已核對 ZIP 僅含本次執行檔，內部執行檔 SHA-256 與建置檔一致，檢查碼附檔與 ZIP 一致。不同建置產物不可只以相同長度判斷內容一致。

- 開啟大圖、快速連續切換、載入參考圖時確認視窗持續回應，最後顯示最新請求；失敗後舊圖仍可編輯。
- 載入期間修改目前圖片，確認新圖交付前要求處理未儲存變更。
- 儲存期間切圖與編輯暫停；選擇儲存並離開時，必須等寫入成功才切圖或關閉。取消另存及寫入失敗均保留工作。
- 調整面板、直方圖、放大鏡、微調區塊與視窗大小，重新啟動後核對設定。最大化時保留先前一般視窗尺寸。
- 在隔離的可攜目錄測試損毀及未來版本設定，確認使用預設值、顯示錯誤，且原設定未被覆寫。

原生桌面操作、流暢度與全新 Windows 環境獨立執行，均需另行驗收；自動化狀態測試與執行檔體積不代表這些項目已通過。
