# 0.8.2 查重大圖檢視修補與交付驗證

日期：2026-11-08。使用者明確授權先審查，再執行 git commit、tag、push。

## 程式碼審查

範圍為查重面板、大圖工具列、主視窗快捷鍵及對應回歸測試。兩份既有未追蹤 Fluent UI 規劃文件不納入交付。

- 大圖模式使用主畫布與既有 `open_image_path`，保留背景解碼、像素限制與未儲存離開保護；工具列只回傳開圖意圖，不預先宣稱載入成功。
- 收起清單不清除掃描結果、選取或刪除勾選；返回按鈕與 Esc 恢復原清單，關閉／重開重設大圖模式。
- 工具列以目前群組首張為基準；比對索引越界時沿用預覽卡片的後備規則，透過安全索引取得資料。
- 大圖模式允許主畫布快捷鍵；離開確認、背景操作及其他對話框的既有優先保護保留。
- 指標回歸涵蓋進入模式、面板隱藏、基準／比對切圖及返回狀態；快捷鍵回歸涵蓋原始尺寸與兩階段 Esc。

未發現阻擋本次交付的問題。此結論限於程式碼與自動化證據，不代表已完成原生桌面驗收或第三方安全稽核。

## 本機檢查

- 五個工作區套件同步 0.8.2；Cargo.lock 僅變更這五個套件版本，第三方依賴不變。
- `cargo check --workspace --offline` 通過。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通過。
- `cargo fmt --all --check` 與 `git diff --check` 通過。
- `cargo test --workspace --locked --offline`：194 項通過、3 項既有忽略。
- `cargo build --release -p app-ui --locked --offline` 通過；封裝腳本與 ZIP 單一 app-ui.exe 內容核對通過，壓縮內容與執行檔雜湊一致。
- 執行檔為 7,273,472 位元組，低於 15,000,000 位元組限制；完整內嵌版本 v0.8.2 核對通過。
- EXE SHA-256：`1a496b9bc9e388b0337117aaf395ab65fae2dd54c13534e1c3940d6516f8a542`。
- ZIP SHA-256：`a048526e54a5c38eb76d3a711ccd3421004c25998dad7bf50da3144ce9cc4914`。

## 驗收與 Git

- 原生桌面縮放／平移、工具列移動／摺疊與未儲存確認仍待人工驗收。
- 起始 HEAD 與遠端 main 為 c3d60c58f45b2d97d84989f75834c3a5c5385631；交付前遠端尚無 v0.8.2。
- 工作流程在 main／v* 推送時執行品質檢查；標籤品質工作通過後建立 GitHub Release 草稿，公開發布並非本輪操作。


## Git 交付完成

- 修補提交 `0648a859df38393fed73102da1ae5d6cd47c0698` 已推送 main；[該提交 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/42404604077) 完整通過後才建立標籤。
- 附註標籤 `v0.8.2` 已推送；標籤物件 `6ec4dc0ba8d1e1f358b65d3cef0233e554e284ed`，遠端解析正確指向上述修補提交。
- [標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/42405356805) 查詢時執行中，尚不宣稱 Release 草稿已建立或公開發布。
- 本節為後續文件提交，保留 v0.8.2 指向已通過 CI 的修補提交。
