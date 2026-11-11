# 0.8.3 縮圖總覽清晰度與瀏覽效能交付驗證

日期：2026-11-11。使用者明確授權 git commit、tag、push。

## 範圍

- 第 1 項：縮圖總覽獨立高解析快取（長邊依圖片區較長邊 × 像素密度選 256～1024 層級、遲滯、動態預算 64～256 MiB、隱藏組取消排隊工作）；膠卷列維持 80px／16 MiB。
- 第 2 項：`ImagePlaylist::ensure_sort` 在排序條件不變時跳過重排；已核對所有清單變更方法皆以目前條件收尾排序。
- 第 3 項：日期索引漸進讀取（64 KiB 首段，僅可確定時停止，其餘補讀至 512 KiB）。
- 第 5、6 項：日期交付單次重建並以開啟中文件恢復選取；總覽提示懸停時才產生。
- 外部審查 P2（全景圖高 DPI 放大）、P3（資源契約未同步）已修正並納入；兩份審查紀錄 [REVIEW_V081_REPORT_RECHECK](REVIEW_V081_REPORT_RECHECK.md)、[REVIEW_GRID_OPTIMIZATION_20261111](REVIEW_GRID_OPTIMIZATION_20261111.md) 一併提交。
- 本機變更於交付前接續遠端 0.8.2（`191827e`）；任務文件衝突僅為兩側新增段落，保留雙方內容。

## 本機檢查

- 五個工作區套件同步 0.8.3；Cargo.lock 僅變更這五個套件版本，第三方依賴不變。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 通過。
- `cargo fmt --all --check` 與 `git diff --check` 通過。
- `cargo test --workspace --locked --offline`：202 項通過、3 項既有忽略。
- `cargo build --release -p app-ui --locked --offline` 通過；封裝腳本與 ZIP 單一 app-ui.exe 內容核對通過。
- 執行檔為 7,280,128 位元組，低於 15,000,000 位元組限制；內嵌版本字串 v0.8.3 核對通過。
- EXE SHA-256：`a42cd4ef68fc4ce197e7f87a45593c9e9eba0b16d50d78e909c52a35b36b6ba4`。
- ZIP SHA-256：`0651cd698b82bc622936fd411c39cc6343fac29a9e3c20752760cf790d6533c7`。

## 待驗收

- 原生桌面：100%／200% DPI 下 3:2、方形、3:1 全景與直幅縮圖清晰度、欄數切換與工作集；大資料夾導覽流暢度；大型相簿日期索引冷／暖快取耗時；總覽懸停提示。
- 未做冷快取量測，不宣稱日期索引或導覽的具體加速倍數。
