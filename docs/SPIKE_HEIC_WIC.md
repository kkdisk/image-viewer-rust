# T0：HEIC／WIC 技術驗證

日期：2026-11-12。對應 [HEIC 規格](SPEC_HEIC_SUPPORT.md)。驗證程式為 scratchpad 中的一次性程式，未進產品程式碼。

## 環境與樣本

- Windows 11 Pro 10.0.26300；HEIF 影像延伸模組 1.2.62、HEVC 影片延伸模組 2.5.48（另有 HEVC Video Extensions 2.4.151）、AV1 影片延伸模組 2.0.35。
- `windows` 0.62.2（功能 `Win32_Graphics_Imaging`、`Win32_System_Com`、`Win32_System_Variant`）、`kamadak-exif` 0.5.5，均為離線建置，沒有新增 crate。
- 樣本：使用者提供的 `temp/20250602_112538943_iOS.heic`（iPhone 14 Pro，2,198,662 bytes，不提交）。只有這一個樣本，下方「未驗證」各項需要更多樣本。

## 容器結構（自寫的有界 box 掃描）

| 項目 | 結果 |
|---|---|
| `ftyp` | 主品牌 `heic`；相容品牌 `mif1 MiHE MiPr miaf MiHB heic` |
| 頂層 box | `ftyp`@0（40）、`meta`@40（4,241）、`mdat`@4,281（2,194,381）：`meta` 在前 |
| 主影像 | item 49，類型 **`grid`**；共 61 個 `hvc1` 分塊、2 個 `grid`、1 個 `Exif`、1 個 `mime`（XMP） |
| 主影像屬性 | `colr prof`（ICC 536 bytes，描述為 **Display P3**）、`ispe 4032×3024`、**`irot 270°`**、`pixi 8/8/8` |
| Exif 位置 | 檔案位移 4,660，長度 2,856：位於檔案很前面 |

結論：兩層判定可行。主影像是 `grid` 時，要看分塊類型（`hvc1`）才能知道實際編碼。

## WIC 解碼

| 項目 | 結果 | 對規格的影響 |
|---|---|---|
| 容器辨識 | `GUID_ContainerFormatHeif`，1 個 frame | — |
| 解碼前取尺寸 | **3024×4032**（已套用 `irot`）；原生像素格式 `32bppBGR` | 像素上限檢查可在解碼前完成 |
| 方向 | WIC **已套用 `irot`**，`System.Photo.Orientation` 正規化為 1；但 `kamadak-exif` 讀到的 EXIF Orientation 仍是 6。目視輸出為正確直式 | **確認會重複旋轉**：HEIF 必須略過 EXIF 方向校正 |
| 色彩內容 | `GetColorContexts` 回傳 **0**，儘管容器有 P3 ICC | WIC 不會提供來源色彩，必須自行從 `colr(prof)` 取出 ICC |
| 色彩轉換 | 以 ICC 位元組 `InitializeFromMemory` 建立來源色彩內容，目標為 Exif 色彩空間 1（sRGB），`IWICColorTransform` 以 32bppRGBA 初始化**成功**。平均飽和度 0.337 → 0.378（P3 轉 sRGB 後變鮮豔，符合預期）；每通道平均差 2.4，最大 68 | 規格 4.1 可行；ICC 改由 T2a 掃描提供 |
| 完整解碼 RGBA8 | 410～495 ms（12MP，三次） | — |
| 加色彩轉換 | 約 725 ms（多約 300 ms） | 文件載入可接受 |
| 內嵌縮圖 `GetThumbnail` | **240×320，約 29 ms** | 縮圖列與 256 層級總覽可大幅加速（T6 有依據） |
| 縮放解碼 `IWICBitmapSourceTransform` | 可用；256×341 約 309 ms、768×1024 約 439 ms | HEVC 分塊仍需完整解碼，**節省有限，不採用** |
| `IWICBitmapScaler` 256×192 | 200～297 ms | 不採用 |
| 記憶體來源 `IWICStream::InitializeFromMemory` | 成功，像素與檔案解碼完全相同 | `load_image_from_memory` 可分流 |
| HEIF 編碼器 | `CreateEncoder(GUID_ContainerFormatHeif)` 可建立 | 測試樣本可在測試執行時產生（元件存在時），不必提交二進位樣本 |

## COM 與執行緒

- 主執行緒以 MTA 初始化後，未初始化的背景執行緒也能建立工廠（加入程序的隱含 MTA）；**這不能依賴**，仍需每執行緒初始化。
- 已是 STA 的執行緒再以 MTA 初始化會回傳 `RPC_E_CHANGED_MODE`（0x80010106），但工廠仍可使用，應視為可用、不要反初始化。
- 背景執行緒以 MTA 初始化後完整解碼成功。

## EXIF 與日期索引

- `kamadak-exif` 解析 HEIF 時，以 `take(len).read_to_end` 讀取區段，**不會依宣告長度預先配置**，另有 `MAX_EXIF_SIZE` 上限；有界 reader 可同時限制讀取量與配置量。
- 有界 reader 實測：預算 512 KiB 與 64 KiB 都只實讀 **16,384 bytes**（4 KiB 緩衝的整數倍），取得 DateTimeOriginal `2025-06-02 19:25:38`、OffsetTimeOriginal `+08:00`、Make `Apple`、Model `iPhone 14 Pro`；預算 8 KiB 時回傳「超過讀取預算」錯誤，沒有 panic。
- 此樣本的 Exif 在檔頭，現行「先讀 64 KiB」的流程也讀得到；但 `mdat` 在 `meta` 之前的檔案仍需 T5 的有界 Seek reader。

## 未驗證（需要更多樣本或環境）

- 缺少延伸模組時的錯誤碼（預期為 `WINCODEC_ERR_COMPONENTNOTFOUND` 0x88982F50；本機已安裝，無法重現）。
- `imir` 鏡像、非 `grid` 單一影像、10 位元、只有 `nclx` 沒有 ICC、深度圖、原況照片、`mdat` 在前的 HEIF、48MP。
- AVIF：沒有樣本；AV1 延伸模組存在，但 WIC 的 AVIF 解碼行為尚未驗證。
- Microsoft Store 產品連結需要確認後才寫入產品程式。

## 對規格的修訂建議（已寫入規格第 3 版）

1. 方向：HEIF 一律略過 EXIF 方向校正，以 WIC 輸出為準。
2. 色彩：ICC 由 T2a 掃描從 `colr(prof)` 取得；只有 `nclx` 且色域不是 sRGB 時，降級為不轉換並標示。
3. 縮圖：縮圖列與 256 層級總覽優先使用內嵌縮圖（長邊足夠時），並對縮圖套用相同的色彩轉換；不採用縮放解碼。
4. 測試樣本：元件存在時，以 WIC 編碼器在測試中產生；真實 iPhone 樣本只用於本機與原生驗收。
5. COM：每執行緒初始化 MTA，`RPC_E_CHANGED_MODE` 視為可用。
