# HEIC／HEIF／AVIF 讀取：交付前驗證

日期：2026-11-12。範圍為 [HEIC 規格](SPEC_HEIC_SUPPORT.md) T1～T7，版本 0.9.0（使用者確認）。

## 環境

- Windows 11 Pro 10.0.26300；HEIF Image Extension 1.2.62、HEVC Video Extensions 2.5.48、AV1 Video Extension 2.0.35。
- 沒有新增 crate：core-image 使用工作區已有的 `windows` 0.62.2；Cargo.lock 只多一條相依邊。

## 自動化

- `cargo clippy --workspace --all-targets --offline -- -D warnings` 通過；`cargo fmt --all --check` 通過。
- `cargo test --workspace --offline`：222 項通過、7 項忽略（3 項既有、4 項需真實樣本）。
- 以 `--ignored 真實` 執行 4 項真實樣本測試（`temp/20250602_112538943_iOS.heic`，不提交），全數通過：
  - core-image：尺寸 3024×4032（`irot 270°` 已套用、未依 EXIF 6 重複旋轉）、P3 ICC 轉 sRGB 後像素與未轉換不同、內嵌縮圖為直式。
  - app-ui 開圖：HEIC 與同目錄 PNG 進入清單、格式「HEIF（HEVC）」、色彩說明、拍攝時間 `2025-06-02 19:25:38.943`。
  - 日期索引：有界讀取取得含小數秒的時間，實讀不超過 64 KiB。
  - 縮圖：80px 與 256 層級使用內嵌縮圖（暖啟動約 47 ms，首次含 COM 初始化約 176 ms），384 層級回退完整解碼。
- 主要新增回歸：
  - 容器探測：品牌與實際編碼分層（`mif1`＋`av01` 判為 AV1）、`grid`／`dimg`、ICC 與 nclx、品牌後備、`mdat` 在前只讀標頭（實讀小於 1 KiB）、截斷與損毀逐位元組不 panic、超大 `meta` 拒絕。
  - WIC 管線：以 PNG 經檔案、記憶體、背景執行緒與 rayon 解碼；以 WIC 編碼器在測試中產生 HEIC，驗證各入口、記憶體來源與像素上限。
  - 儲存：唯讀來源改開另存並預設同名 `.jpg`、取消不改變狀態、另存 JPEG 後原檔不變、拒絕 `.heic`／`.heif`／`.avif` 目標（app 與核心兩層）、儲存並繼續的取消與成功兩種路徑。
  - 日期：Exif 位於 4 MiB 影像資料之後仍以 Seek 取得且實讀小於 64 KiB；宣告超大長度時讀取量不超過 512 KiB；截斷檔不 panic。
  - 安裝指引：依編碼列出延伸模組（AVIF 不含 HEVC）、Store URI、視窗顯示與關閉。
  - 格式清單：可寫皆可讀、唯讀格式不在可寫清單、GIF 不屬唯讀、core-archive 清單一致。
- `cargo build --release -p app-ui --locked --offline` 通過；執行檔 7,344,640 位元組（0.8.3 為 7,280,128，增加 64,512 位元組），低於 15,000,000 位元組限制。

## 0.9.0 交付檢查

- 五個工作區套件同步 0.9.0；Cargo.lock 只變更這五個套件版本與 core-image → windows 相依邊，第三方依賴版本不變。
- 改版後重跑：Clippy（`--locked`）、格式、`cargo test --workspace --locked --offline`（222 項通過、7 項忽略）皆通過。
- 發布建置與 `scripts/package_windows.ps1` 通過；ZIP 只含 `app-ui.exe`，內嵌版本字串 v0.9.0 核對通過。
- EXE SHA-256：`efe03c00fb01a2490729439a05bf2a8f26f9fd42b0b5fce4fe2293bc7d8fc66d`。
- ZIP SHA-256：`d37be57dd4953e11f23671db1c50b5333c6644b4201a99702fe38eb1df584f00`。

## 原生桌面

- 使用者以實際相簿 34 張 iPhone 14 Pro HEIC 驗收（2026-11-12 截圖）：直式與橫式方向、色彩說明、格式、相機、含時區拍攝時間、縮圖列、依拍攝時間排序皆正常。
- 待驗：T4～T6 變更後的原生操作（儲存改另存、縮圖速度）、縮圖總覽、查重、ZIP 內 HEIC、缺少延伸模組提示（本機已安裝，無法重現）、AVIF（無樣本）。

## 外部資訊核對

- Microsoft Store 產品頁 `9PMMSR1CGPWG`、`9NMZLZ57R3T7`、`9MVZQVXJBQ9V` 存在，標題分別為「HEIF Image Extension」「HEVC Video Extensions」「AV1 Video Extension」；發行者與價格未取得。

## CI 限制

- GitHub Actions 的 Windows 執行器不保證安裝 HEIF／HEVC 延伸模組：以 WIC 編碼器產生樣本的測試在無編碼元件時會略過並輸出提示，真實樣本測試為 `#[ignore]`。CI 通過不代表 HEVC 解碼已在該環境驗證。

## Git 交付完成

- 交付提交 `7f28c42c23b25fbe7a9ff244d8fc91e784c66d09` 已推送 main（前一筆 `d8ba74f` 為同版 T1～T3）；[該提交 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/43064190772) 完整通過後才建立標籤。
- 附註標籤 `v0.9.0` 已推送；標籤物件 `266ae8a30cd803d71e6fbca90e746cbb62726e98`，遠端解析正確指向上述提交。
- [標籤 CI](https://github.com/kkdisk/image-viewer-rust/actions/runs/43065860610) 查詢時執行中，尚不宣稱 Release 草稿已建立或公開發布。
- 本節為後續文件提交，保留 v0.9.0 指向已通過 CI 的交付提交。
