# 0.9.1 驗證紀錄

日期：2026-11-12。範圍：「檔案 → 最近開啟」新增「清除最近清單」，使用者授權直接發布 0.9.1。

## 變更

- 偏好設定新增 `recent_cleared_ms` 清除時間點（舊設定檔預設 0，不需轉換）；讀取與鎖內合併共用同一過濾規則，開啟時間不晚於清除時間點的項目與移除紀錄一律捨棄。
- 清除時間點不早於現有任何項目時間；之後新增或移除項目的時間一律晚於清除時間點。清除後立即排入寫入。

## 本機檢查

- 五個工作區套件同步 0.9.1；Cargo.lock 只變更這五個套件版本。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、`cargo fmt --all --check` 與 `git diff --check` 通過。
- `cargo test --workspace --locked --offline`：223 項通過、7 項忽略（3 項既有、4 項需真實 HEIC 樣本）。新增回歸：舊視窗快照的一般與關閉寫入不能復活項目、未來時間的舊項目也被清除、清除後新開項目照常記錄。
- 開發期間首次全量測試曾遇新測試連續原子替換時 Windows「存取被拒」（暫時性檔案占用）；之後偏好設定測試連跑 5 次及兩次全量測試皆通過。
- 發布建置與 `scripts/package_windows.ps1` 通過；ZIP 只含 `app-ui.exe`，內嵌版本字串 v0.9.1 核對通過。
- 執行檔 7,348,224 位元組，低於 15,000,000 位元組限制。
- EXE SHA-256：`7452fe2f03f054bddda8d452841a64769fdc74bb89f07eddb52f537bfa07acb7`。
- ZIP SHA-256：`9ce9c5d8c7c0543b720d40c5110ab5de4b954daf1cd33c9ced258f8aa48cfad4`。

## 原生桌面

- 使用者於 2026-11-12 回報清除最近清單驗證通過。

## Git 交付完成

- 交付提交 `ddabdcd62efdbba9a7cbcdedc2f8f753de59c1f6` 已推送 main；[該提交 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/43068341421) 完整通過後才建立標籤。
- 附註標籤 `v0.9.1` 已推送；標籤物件 `d761732f4e8b776771b99e2481506bb96e3a450e`，遠端解析正確指向上述提交。
- [標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/43069677953) 查詢時執行中，尚不宣稱 Release 草稿已建立或公開發布。
- 本節為後續文件提交，保留 v0.9.1 指向已通過 CI 的交付提交。
