//! 缺少系統解碼元件（HEIF／HEVC／AV1 延伸模組）時的安裝指引視窗。
use crate::app::ImageViewerApp;
use core_image::heif::Codec;

/// Microsoft Store 產品（名稱、產品 ID）；ID 已以官方商店頁面標題核對（2026-11-12）。
const HEIF_EXTENSION: (&str, &str) = ("HEIF Image Extension", "9PMMSR1CGPWG");
const HEVC_EXTENSION: (&str, &str) = ("HEVC Video Extensions", "9NMZLZ57R3T7");
const AV1_EXTENSION: (&str, &str) = ("AV1 Video Extension", "9MVZQVXJBQ9V");

/// 依缺少的編碼列出需要的延伸模組，依建議安裝順序排列。
pub(crate) fn required_extensions(codec: Codec) -> &'static [(&'static str, &'static str)] {
    match codec {
        Codec::Hevc => &[HEIF_EXTENSION, HEVC_EXTENSION],
        Codec::Av1 => &[AV1_EXTENSION, HEIF_EXTENSION],
        Codec::Other(_) | Codec::Unknown => &[HEIF_EXTENSION],
    }
}

pub(crate) fn store_uri(product_id: &str) -> String {
    format!("ms-windows-store://pdp/?ProductId={product_id}")
}

/// 交給系統開啟商店頁面；不新增依賴。
fn open_store(product_id: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(store_uri(product_id))
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = product_id;
        Err("此平台不提供 Microsoft Store".into())
    }
}

impl ImageViewerApp {
    pub(crate) fn show_codec_help(&mut self, ctx: &egui::Context) {
        let Some(codec) = self.codec_help else {
            return;
        };
        let mut open = true;
        let mut failure = None;
        egui::Window::new("需要安裝解碼元件")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!(
                    "這張圖片使用 {} 編碼，Windows 需要下列延伸模組才能開啟：",
                    codec.label()
                ));
                ui.add_space(6.0);
                for (name, id) in required_extensions(codec) {
                    if ui.button(format!("在 Microsoft Store 開啟：{name}")).clicked()
                        && let Err(error) = open_store(id)
                    {
                        failure = Some(format!("無法開啟 Microsoft Store：{error}"));
                    }
                }
                ui.add_space(6.0);
                ui.weak("安裝完成後重新開啟圖片即可；原始檔案不會被修改。部分延伸模組可能需付費或由電腦製造商預裝。");
            });
        if let Some(message) = failure {
            self.status_notification = Some((message, std::time::Instant::now()));
        }
        if !open {
            self.codec_help = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 依編碼列出正確的延伸模組且不會叫_avif_安裝_hevc() {
        let names = |codec| {
            required_extensions(codec)
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(Codec::Hevc),
            ["HEIF Image Extension", "HEVC Video Extensions"]
        );
        assert!(!names(Codec::Av1).contains(&"HEVC Video Extensions"));
        assert_eq!(names(Codec::Av1)[0], "AV1 Video Extension");
        assert_eq!(
            store_uri("9PMMSR1CGPWG"),
            "ms-windows-store://pdp/?ProductId=9PMMSR1CGPWG"
        );
    }

    #[test]
    fn 視窗顯示後可關閉且不影響文件() {
        let ctx = egui::Context::default();
        let mut app = ImageViewerApp::default();
        app.codec_help = Some(Codec::Hevc);
        // 視窗首幀只量測尺寸，第二幀才有文字形狀。
        let _ = ctx.run(Default::default(), |ctx| app.show_codec_help(ctx));
        let output = ctx.run(Default::default(), |ctx| app.show_codec_help(ctx));
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text().contains("HEVC Video Extensions")
        )));
        app.codec_help = None;
        let output = ctx.run(Default::default(), |ctx| app.show_codec_help(ctx));
        assert!(output.shapes.is_empty());
        assert!(app.session.is_none());
    }

    /// 真實 iPhone 樣本的完整開圖流程；樣本放在 temp/ 不提交。
    /// 執行：cargo test -p app-ui -- --ignored 真實_heic
    #[test]
    #[ignore = "需要 temp/ 內的真實 HEIC 樣本與 HEIF／HEVC 延伸模組"]
    fn 真實_heic_開圖進入清單並顯示格式與色彩() {
        let sample = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../temp/20250602_112538943_iOS.heic");
        let dir = tempfile::tempdir().unwrap();
        let heic = dir.path().join("相片.HEIC");
        std::fs::copy(&sample, &heic).unwrap();
        image::RgbaImage::new(2, 3)
            .save(dir.path().join("其他.png"))
            .unwrap();
        let mut app = ImageViewerApp::default();
        app.open_file_path(heic.clone()).unwrap();
        assert!(app.wait_document_io(std::time::Duration::from_secs(10)));
        let session = app.session.as_ref().expect("HEIC 應開啟成功");
        assert_eq!(session.source_path(), Some(heic.as_path()));
        assert_eq!(
            session.active_image().dimensions(),
            (3024, 4032),
            "直式且不重複旋轉"
        );
        assert_eq!(app.playlist.len(), 2, "同目錄 HEIC 與 PNG 都進入清單");
        assert_eq!(app.convenience.info.format, "HEIF（HEVC）");
        assert_eq!(
            app.convenience.info.color.as_deref(),
            Some("已依內嵌 ICC 轉換為 sRGB")
        );
        assert_eq!(
            app.convenience.info.photo.captured_at.as_deref(),
            Some("2025-06-02 19:25:38.943")
        );
        assert!(app.codec_help.is_none());
    }
}
