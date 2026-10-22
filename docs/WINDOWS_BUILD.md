# Windows 建置與驗收

## 本機驗證

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
cargo build --release -p app-ui --locked --offline
./scripts/package_windows.ps1
```

首次下載相依套件時移除 `--offline`。`Cargo.lock` 固定應用程式相依版本，應納入版本控制。

封裝腳本以 Cargo 中的應用程式版本命名 ZIP，內含 `app-ui.exe`，並另外產生 ZIP 的 SHA-256 檢查碼。執行檔必須小於 15 MiB；產物放在已忽略的 `dist/`。

## 自動流程

[Windows 流程](../.github/workflows/windows.yml) 在拉取請求、推送至 `main`、推送 `v*` 標籤及手動觸發時執行格式、Clippy、工作區測試、發布建置及封裝。使用 Windows runner 與穩定版 Rust；一般工作只有儲存庫讀取權限。建置產物保留 14 天，可依 [GitHub 官方說明](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/download-workflow-artifacts) 下載。

標籤推送額外核對標籤必須等於應用程式版本的 `v` 前綴形式，例如套件 `0.1.0` 對應 `v0.1.0`。品質工作成功後，獨立的發布工作取得本次產物，以 `contents: write` 建立附有 ZIP 與檢查碼的 GitHub Release 草稿。草稿須由維護者確認桌面驗收結果後發布。重跑既有標籤不覆寫既有 Release；已存在時會回報失敗，應先查明原草稿及產物狀態。

流程的 checkout 與產物上傳介面依 [checkout 官方文件](https://github.com/actions/checkout) 與 [upload-artifact 官方文件](https://github.com/actions/upload-artifact) 設定。本機測試不能視為 GitHub runner 實跑通過；推送後應另行查閱對應提交與標籤的工作結果。

## 桌面驗收

- 開啟大圖、快速連續切換、載入參考圖時確認視窗持續回應，最後顯示最新請求；失敗後舊圖仍可編輯。
- 載入期間修改目前圖片，確認新圖交付前要求處理未儲存變更。
- 儲存期間切圖與編輯暫停；選擇儲存並離開時，必須等寫入成功才切圖或關閉。取消另存及寫入失敗均保留工作。
- 調整面板、直方圖、放大鏡、微調區塊與視窗大小，重新啟動後核對設定。最大化時保留先前一般視窗尺寸。
- 在隔離的可攜目錄測試損毀及未來版本設定，確認使用預設值、顯示錯誤，且原設定未被覆寫。

原生桌面操作、流暢度與全新 Windows 環境獨立執行，均需另行驗收；自動化狀態測試與執行檔體積不代表這些項目已通過。
