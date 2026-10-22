# v0.5.x 看圖便利操作補充契約

沿用主規格第 8 節及 ROADMAP 第 7、11 節；本批已授權實作，套件版號為 0.5.1。六項功能分別驗證，原生桌面驗收不由單元測試代替。

| 功能 | 入口與行為 | 主要驗證 |
|---|---|---|
| 唯讀資訊 | 檢視選單；來源、格式、大小、UTC 修改時間、載入／目前尺寸及動畫標示。檔案資料由背景交付，另存後更新 | 不變更像素、歷史或原檔；Unicode 路徑、閏年時間 |
| 旋轉／翻轉 | 編輯選單；順／逆時針 90 度、水平／垂直翻轉；未套用預覽先拒絕並提示，背景運算完成提交 Session 一個節點 | 完整尺寸、Alpha、Undo／Redo、已儲存節點與過期結果 |
| JPEG 儲存 | 一律確認重新壓縮；動畫另列轉靜態警告；沿用相同確認快照與離開保護 | 取消不寫入，確認後走背景原子存檔 |
| 重新整理 | F5／檢視選單重掃及重載；未儲存先詢問。取得焦點只重掃同資料夾，保留工作與選取；來源外部移除時保留畫面並提示，可另存或選其他檔 | 新增／移除檔、髒工作、取消／放棄、過期掃描隔離 |
| 幻燈片 | 空白鍵／檢視選單，全螢幕；2／3／5／10 秒（預設 3），循環開、隨機關；從目前圖片起，以完成載入時間計時 | 可注入時間、非循環結束、失敗略過計數、不重試已知壞檔；手動導覽、停止及 Esc 使待交付圖片失效 |
| 刪除目前圖片 | Delete／檢視選單；先處理離開保護，再確認回收；「不再詢問」僅確認後生效，選單可恢復 | 取消不動原檔；背景回收完成才移除清單並選下一張，最後一張刪除後清空；失敗保留工作與選取 |
| 複製路徑 | Ctrl+Shift+C／編輯選單；一般來源複製絕對路徑，壓縮來源複製 `壓縮檔路徑 :: 內部相對路徑` | 使用既有 egui 剪貼簿，不新增影像剪貼簿套件，不暴露暫存路徑 |

壓縮檔來源的 F5 重掃及目前圖片刪除停用；不修改或重解壓既有壓縮 Session。資訊視窗在全螢幕隱藏，退出後保留開啟狀態。快捷鍵說明及派送共用定義，文字輸入不觸發新按鍵。

## 回收安全邊界

本批目前圖片刪除使用 `core-archive::recycle::recycle_file`。Windows 使用已鎖定的 `windows`／`windows-core` 0.62.2 加為直接依賴，沒有新增套件版本、C/C++ 套件或外部非系統 DLL。原定重用 `trash` 的實測未能核對回收項目，故加強為系統刪除前回呼檢查。

Windows 系統回報不能回收時，在 `PreDeleteItem` 回傳錯誤取消動作；只允許一般檔案，不接受資料夾或符號連結。要求回收、無介面及遇錯停止，並核對回呼是否執行、單檔結果及整體取消狀態。其他平台在尚未驗證前拒絕刪除；沒有永久刪除備援。這不是編輯 Undo，可回收時應由作業系統還原。

回呼取消依 [Microsoft PreDeleteItem 契約](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperationprogresssink-predeleteitem)；無法回收時檢查旗標並拒絕的作法可參照 [Electron 原始實作](https://github.com/electron/electron/blob/main/shell/common/platform_util_win.cc)。現有查重工具的回收入口不屬於這次目前圖片刪除實作，後續應另行套用及驗證同一保護。

## 驗收邊界

本機實測回收不可用時拒絕操作，原檔位元組及清單保持不變；不能據此宣稱成功回收／還原已通過。原生桌面仍需逐項操作：資訊、旋轉後復原／重做與 JPEG 警告、F5 與焦點重掃、幻燈片計時與退出、Delete 確認／選項恢復／成功回收與系統還原、剪貼簿貼入檢查。

驗證命令：

```powershell
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
cargo build --release -p app-ui --locked --offline
cargo test -p app-ui --lib --locked --offline native_recycle_roundtrip_or_refusal_preserves_fixture -- --ignored --nocapture
```
