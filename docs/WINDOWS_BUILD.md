# Windows 建置與驗收

## 0.8.0 審查修正版（2026-10-23，最新本機產物）

修正日期狀態分歧及標籤文字複製被圖片覆蓋，附四項回歸。188 項測試通過、3 項預設忽略，Clippy／格式／差異與發布建置通過；最新執行檔 `target/release/app-ui-080-review.exe`，7,267,328 位元組，隔離啟動十秒未提前退出。ZIP 已更新，雜湊見 [驗證紀錄](VALIDATION_V080.md)，其餘建議處置見 [審查紀錄](REVIEW_V080.md)。原生操作與 CI 待驗，未提交或推送。

## 0.8.0 EXIF 拍攝相機（2026-10-23，最新本機產物）

圖片資訊有資料時顯示 EXIF Make／Model。184 項測試通過、3 項預設忽略，Clippy／格式／差異與發布建置通過。最新執行檔 `target/release/app-ui-080-exif.exe`，7,264,768 位元組；隔離啟動十秒未提前退出。ZIP 已更新，雜湊見 [驗證紀錄](VALIDATION_V080.md)。原生相機顯示與 CI 待驗，未提交或推送；先前剪貼簿使用者驗收紀錄保留。

## 0.8.0 影像剪貼簿（2026-10-23，最新本機產物）

新增背景影像複製及功能檢查清單；183 項測試通過、3 項預設忽略，Clippy／格式／差異與發布建置通過。最新執行檔 `target/release/app-ui-080-clipboard.exe`，7,265,792 位元組；隔離啟動十秒未提前退出。0.8.0 ZIP 已更新為此版；完整雜湊見 [驗證紀錄](VALIDATION_V080.md)，功能與後續規劃見 [清單](FEATURE_CHECKLIST.md)。使用者於 2026-10-23 回報「影像剪貼簿 ok」，整體原生驗收通過；CI 待驗，未提交或推送。

## 0.8.0 日期工具列精簡（2026-10-23，最新本機產物）

日期操作改成彈出面板，單列高度 40 邏輯像素。179 項測試通過、3 項預設忽略；Clippy／格式／差異與發布建置通過，隔離啟動十秒未提前退出。最新執行檔 `target/release/app-ui-080-compact.exe`，7,238,144 位元組；0.8.0 ZIP 已更新為此版，完整雜湊與待驗項目見 [驗證紀錄](VALIDATION_V080.md)。下方初版產物保留為歷史。原生桌面與 CI 待驗，尚未提交或推送。

## 0.8.0 日期與縮圖總覽（2026-10-23，本機開發版）

178 項測試通過、3 項預設忽略；Clippy、格式、差異檢查與發布建置通過。新版另存 `target/release/app-ui-080.exe`，7,233,024 位元組，避免覆寫使用者正在執行的舊程式；隔離設定啟動十秒未提前退出。ZIP 與執行檔逐位元組核對一致。完整雜湊、測試與原生桌面待驗項目見 [0.8.0 驗證](VALIDATION_V080.md)。尚未提交、標記或推送，CI 待驗／Unknown。

## 0.7.0 Git 與 CI 交付（2026-10-23）

- 程式碼提交 `c04d0243b4a9ada335cf6e1fecd1d2b3b7f8c60a` 已推送至 origin/main；[main CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39961031532) 為 completed/success，包含格式、Clippy、工作區測試、發布建置與封裝。
- 已建立並推送附註標籤 `v0.7.0`，標籤物件 `aa9efce793addbfee54103522d47dca78c5c4850`，遠端解析已核對為上述程式碼提交。
- [標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39962631717) 已核對 completed/success，head 與標籤提交一致；工作流程只建立 Release 草稿，不等同公開發布。
- 本輪修正與驗證見 [審查紀錄](REVIEW_V070.md)。使用者已驗收平滑曲線桌面操作；本輪兩項保護以新增回歸測試驗證，本機共 170 項通過、3 項忽略。
- 0.8.0 批次處理已依使用者指示暫緩，使用者已授權依[日期與網格契約](SPEC_PHOTO_BROWSER.md)接續實作。

## 0.7.0 審查修正版（2026-10-23，最新本機產物）

- 已完成 [程式碼審查](REVIEW_V070.md)，重現並修正來源路徑別名繞過預設集原圖保護、預設集操作期間可提前關閉兩項問題。
- 本輪工作區 170 項通過、3 項預設忽略；全工作區 Clippy、格式及差異檢查通過。使用者已回報平滑曲線版桌面 pass；兩項審查修正以新增自動化回歸驗證。
- 正常 `cargo build --release -p app-ui --locked --offline` 成功，`target/release/app-ui.exe` 已更新為本次審查修正版，7,158,272 位元組，EXE SHA-256 `0b9539b996dc2e1bc20c98b2636c2f9c95bb067c410c81744a07aa87095510e4`。
- 正常封裝腳本產生 ZIP 3,467,624 位元組，僅含與本次執行檔逐位元組相同的 app-ui.exe，版本為 0.7.0；ZIP SHA-256 `fc419324fc9122a7406fa5388da2cce388085061d67fd7a08743865600eb70d6` 且附檔一致。
- 隔離設定隱藏啟動觀察十秒，未提前退出，只終止本輪測試建立的行程。已授權提交、標籤及推送；CI 待提交後核對。尚未公開發布。
- 依使用者最新指示，0.8.0 批次方向暫緩，非批次候選見 [評估文件](EVALUATION_V080.md)，未開始實作。

## 0.7.0 平滑曲線修正（2026-10-23，審查前歷史產物）

- 預設改為保形三次平滑插值，圖形與 LUT 共用取樣；保留折線模式與演算法 1 舊預設集，新平滑預設集採演算法 2。
- 工作區 168 項通過、3 項預設忽略；全工作區 Clippy、格式及差異檢查通過，新增平滑銜接、控制點／無過衝、舊預設集效果相容測試。
- 使用者仍在執行原 `target/release/app-ui.exe`；以 `cargo rustc --release -p app-ui --bin app-ui --locked --offline -- --emit=link=D:/Code/AI_generate_Code/image-viewer-rust/target/release/app-ui-smooth.exe` 成功另產生新檔，未中止或覆寫使用者原行程。
- 新執行檔 `target/release/app-ui-smooth.exe` 為 7,156,736 位元組，SHA-256 `d77a2a9dea68d5ddd78cac2558760a33297d4f08768c74ca7c6d8719c2b5b104`，確認內含完整 0.7.0 版本及平滑／折線文字。隔離設定啟動十秒未提前退出，只終止本輪建立的測試行程。
- 已重新封裝 `dist/rust-image-viewer-0.7.0-windows-x64.zip`，3,468,173 位元組；ZIP 僅含 `app-ui.exe`，內容逐位元組等同新平滑版，SHA-256 `f014040443e964558302f8816a3c769606146a88006b3b0c1d141b869d4f7ef7` 且附檔一致。此次以明確的新執行檔封裝，未使用仍指向舊檔的預設封裝入口。
- 使用新版請啟動 `app-ui-smooth.exe` 或解壓上述 ZIP。原 `target/release/app-ui.exe` 仍是先前版本；關閉該程式後正常 `cargo run --release -p app-ui` 可重新建置原路徑。使用者於 2026-10-23 明確回報此平滑曲線版原生桌面驗證通過（pass）；0.7.0 CI 仍待驗／Unknown，未提交或推送。

## 0.7.0 曲線與預設集開發版（2026-10-23，平滑修正前產物）

- 五套件及鎖定檔均為 0.7.0；工作區 165 項通過、3 項預設忽略，全工作區 Clippy、格式及差異檢查通過。
- 新測試涵蓋曲線 LUT、組合順序、Alpha、非法控制點、多 DPI 指標新增／拖曳、預設集重讀、過期匯入、覆寫與鎖定目標保護、單一套用／復原／重做及輸出像素、快速更新與取消。
- `cargo build --release -p app-ui --locked --offline` 成功；執行檔 7,151,616 位元組，確認含完整 0.7.0 版本文字，小於 15,000,000 位元組。EXE SHA-256 `940bcbed764ee3706b9b1c70fd5aa1b43be8f4434173cf9345a99b77bbeb43d7`。
- ZIP `dist/rust-image-viewer-0.7.0-windows-x64.zip` 3,465,900 位元組，僅含與本次執行檔逐位元組相同的 `app-ui.exe`，SHA-256 `8ee85efedf95447357d4ee2bf4f9524207764ea449e23de84ea0f41fd031718a`，與附檔一致。
- 使用隔離設定隱藏啟動觀察十秒，未提前退出；只終止本輪建立的測試行程。原生桌面與 0.7.0 CI 待驗／Unknown；本版尚未提交、標籤或推送。
- 實作與操作見 [調色契約](SPEC_TONING_PRESETS.md)。下方 0.6.1 執行檔數據屬歷史紀錄，該版保留於獨立 ZIP。

## 0.6.1 Git 推送核對（2026-10-23）

使用者接受目前排版並授權提交／標籤／推送。發布提交 `c73b27acc90e6c3b5cef531b090db8450add4f66` 已推至 origin/main；[main CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39950223153) 為 completed/success。其後建立並推送附註標籤 `v0.6.1`（標籤物件 `be796370a4a5580702f3843eda1e9d3b5e3b322e`），以 ls-remote 核對標籤解析與遠端 main 均為該提交。已沿用既有 GitHub 隱私郵件地址，無變更帳號隱私保護設定。

[標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39951457835) 另行核對；工作流程僅建立 Release 草稿，不公開發布。0.7.0 為後續未提交開發內容，不屬於此標籤。

## 0.6.1 介面排版核對（2026-10-23，歷史產物）

- 五套件與鎖定檔均為 0.6.1；執行檔確認含完整版本文字。
- 完整工作區測試 154 項通過、3 項預設忽略；全工作區 Clippy、格式及差異檢查通過。介面案例涵蓋通知切換、最小視窗及高 DPI、分頁保留草稿與畫布、縮圖拖曳及高度還原、舊設定欄位相容。
- `cargo build --release -p app-ui --locked --offline` 成功；`target/release/app-ui.exe` 為 7,068,672 位元組，小於 15,000,000 位元組。EXE SHA-256：`6c9c1936249363799c49a39d5d73b5d776a5fe74cc7ce9279acf37db4119e364`。
- ZIP `dist/rust-image-viewer-0.6.1-windows-x64.zip` 為 3,430,882 位元組，僅含與本次執行檔逐位元組相同的 `app-ui.exe`；SHA-256 `93d32921a50c55fbde2fa81ab4d60925596c0b0dbca11697b9896374bc3c30a4`，與檢查碼檔一致。
- 使用隔離 APPDATA 設定隱藏啟動新版觀察十秒，未提前退出；只終止本輪測試建立的行程。這不替代原生互動驗收。
- 使用者於 2026-10-23 提供新版畫面並回報排版可接受，授權提交、標籤及推送；未逐項操作的桌面案例仍待驗。遠端 CI 待核對，未公開發布；實作範圍見 [UI 排版契約](UI_LAYOUT_REVIEW.md)。下方 0.6.0 為歷史紀錄。

## 通知列跳動修補（2026-10-23，最新原始碼）

固定通知列高度，新增通知切換的畫布穩定性測試；app-ui 介面測試 7 項通過，app-ui 全目標 Clippy、格式及差異檢查通過。發布重建於替換既有 `target/release/app-ui.exe` 時遭 Windows 拒絕存取（os error 5），因此未完成此次發布重建或重新封裝。下方 0.6.0 執行檔及 ZIP 數據屬修補前產物，不含本次修補。請關閉仍在執行的圖片瀏覽器後重新建置。

## 0.6.0 本機開發版核對（2026-10-23）

- 五個工作區套件與鎖定檔均為 0.6.0，執行檔包含完整版本文字。
- `cargo test --workspace --locked --offline`：150 項通過、3 項依原設定忽略。包含真實 JPEG EXIF、拍攝時間、八種方向、輸出不重複旋轉、不同縮放／DPI 的裁切角點拖曳、尺寸預覽及歷史預算。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、`cargo fmt --all -- --check` 與差異檢查通過；發布組態的輸出測試另有 3 項通過，包含 Windows 目標被鎖定時保留原檔。
- `cargo build --release -p app-ui --locked --offline` 成功；`target/release/app-ui.exe` 為 7,064,576 位元組，低於 15,000,000 位元組門檻。
- `dist/rust-image-viewer-0.6.0-windows-x64.zip` 為 3,428,713 位元組，只包含 `app-ui.exe`，內容雜湊與重建執行檔一致。ZIP SHA-256：`09adc8d37edf792d13557d279e3b5b44a243d866027658ccfba9f8cdd9c2bbdb`。
- 使用隔離的 APPDATA 設定隱藏啟動觀察十秒，未提前退出；這不是互動桌面、GPU 效能或原始 63,198 檔案查重驗收。
- 原生桌面與遠端 CI 為待驗／Unknown；未建立新標籤、提交或發布。裁切、尺寸、匯出及實際相機照片的資訊／方向請依 [0.6.0 契約](SPEC_EDITING_OUTPUT.md) 驗收。

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

### 0.4.0／0.5.0 整合開發版

套件整合為 0.5.0，維持 Rust 1.99.1。資源及瀏覽契約見 [補充規格](SPEC_BROWSING_RESOURCES.md)，固定硬體／測資、CPU 交付容量與延遲取捨見 [資源量測](RESOURCE_MEASUREMENTS.md)。原生冷啟動、GPU 幀時間與桌面流程均未由自動化測試取代。

待桌面確認：同目錄縮圖重用與切目錄、外部更新後的縮圖重驗、長條圖縮放與平移細節、主圖／原圖／參考圖及放大鏡座標、完整尺寸另存、Unicode 命令列與資料夾、排序選取、最近項目失效提示、F11／Esc 與兩秒浮層、雙視窗一般設定由最後關閉者提交。排序日期對壓縮來源採解壓檔的背景資料快照。

本機完整重驗曾受 Windows 應用程式控制原則（os error 4551）阻擋，未將未執行項目計為通過；以獨立開發分支 Windows workflow_dispatch 補齊完整回歸。正式本機產物及 CI 結果核對後續記，不新增 0.4.0／0.5.0 標籤或公開 Release。

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

### v0.3.1 遠端完成證據

發布提交 `8121b3f1d0e02ae07f2fb51a4ff73a97ae6dafec` 的 [main CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39780206671) 已成功，下載日誌確認 92 項通過、1 項忽略；[標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/39781163318) 的品質與建立草稿工作均成功，補足本機完整重驗受原則阻擋的證據。

[Release 草稿](https://github.com/kkdisk/image-viewer-rust/releases/tag/untagged-e5e398f633974d4f1361) 編號 420721832，`draft: true`。遠端 ZIP 為 3,316,292 位元組，SHA-256 為 `b0dc6e339257d9a0d4f77be09de276dfc7dd4cf22cabeb75a0b882b6042efbce`，已實際下載並與 API 摘要及 `.sha256` 附檔核對一致。ZIP 只含 `app-ui.exe`，長度 6,651,392 位元組，含完整 `版本 v0.3.1 (Rust 2024 Edition)`；未執行下載內容。此為 CI 產物，與本機重建產物分別記錄，不推論位元組應相同。原生桌面驗收尚未完成，草稿未公開。

- 開啟大圖、快速連續切換、載入參考圖時確認視窗持續回應，最後顯示最新請求；失敗後舊圖仍可編輯。
- 載入期間修改目前圖片，確認新圖交付前要求處理未儲存變更。
- 儲存期間切圖與編輯暫停；選擇儲存並離開時，必須等寫入成功才切圖或關閉。取消另存及寫入失敗均保留工作。
- 調整面板、直方圖、放大鏡、微調區塊與視窗大小，重新啟動後核對設定。最大化時保留先前一般視窗尺寸。
- 在隔離的可攜目錄測試損毀及未來版本設定，確認使用預設值、顯示錯誤，且原設定未被覆寫。

原生桌面操作、流暢度與全新 Windows 環境獨立執行，均需另行驗收；自動化狀態測試與執行檔體積不代表這些項目已通過。

## v0.5.x 便利操作本機驗證

- 最終全工作區 127 項通過、3 項預設忽略（資源量測、7-Zip 發布效能量測、原生回收桶往返）。
- Clippy 零警告、格式與差異檢查通過；發布建置成功，套件暫留 0.5.0。
- `target/release/app-ui.exe`：6,877,184 位元組；SHA-256 `182dd5c3d0a4f2ab1eb637ea640b8bbcb7fab990e4f7c07d7641bec765726a77`。
- 已核對二進位包含資訊、JPEG 警告及幻燈片文字。本輪未封裝或發布。
- 額外執行原生回收測試：本環境回收不可用時拒絕刪除、原檔與清單保留；成功回收與系統還原仍待桌面驗收。六項操作的待驗項目見 SPEC_VIEWING_CONVENIENCE。
- 本輪日誌：`target/v05x-tests.log`、`target/v05x-clippy.log`、`target/v05x-release.log`（均為本機忽略產物）。

## 0.5.1 版號修正與重建

五個工作區套件及鎖定檔同步為 0.5.1；關於視窗實際繪製回歸、格式及差異檢查通過。發布建置成功，執行檔 6,877,184 位元組，包含完整 `版本 v0.5.1 (Rust 2024 Edition)`，舊 v0.5.0 完整版本文字不存在。

封裝 `dist/rust-image-viewer-0.5.1-windows-x64.zip` 為 3,416,687 位元組，僅含與本次重建相同的 `app-ui.exe`；SHA-256 `d51b22049c28d8316ae91cc8877784913aa9cb59756033c4e9a38f2c2b18c95c`，與檢查碼檔一致。原生桌面驗收仍待完成。

## 0.5.2 高品質放大本機交付

- Rust 1.99.1 工作區 135 項通過、3 項預設忽略；Clippy 零警告、格式及差異檢查通過。日誌：`target/v052-tests.log`、`target/v052-clippy.log`、`target/v052-release.log`。
- 五個套件及鎖定檔為 0.5.2；發布執行檔 6,905,856 位元組，含完整 `版本 v0.5.2 (Rust 2024 Edition)`，舊 v0.5.1 完整版本文字不存在。
- ZIP `dist/rust-image-viewer-0.5.2-windows-x64.zip` 為 3,430,911 位元組，僅含與發布建置相同的 `app-ui.exe`；SHA-256 `4954f880cc23f27a35be6eb7fc6d1a313103ce0bb301728b84456db9671f976e`，與檢查碼檔一致。
- [取樣契約](SPEC_HIGH_QUALITY_ZOOM.md) 已同步。原生照片畫質、不同 DPI 與拖曳流暢度仍待桌面驗收；未以自動化推論已達 60 FPS。
