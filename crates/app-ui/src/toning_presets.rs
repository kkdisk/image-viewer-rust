//! 版本化調色參數；檔案讀寫與替換皆於背景執行。
use crate::app::ImageViewerApp;
use core_image::{
    AdjustmentParams,
    curves::{Curve, CurveInterpolation, CurvePoint, CurveSet},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    time::Instant,
};
const MAX_PRESET_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    version: u32,
    algorithm: u32,
    name: String,
    temperature: i32,
    tint: i32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    curves: [Vec<[u8; 2]>; 4],
}
impl Preset {
    fn new(name: String, params: AdjustmentParams) -> Self {
        let curves = [
            params.curves.rgb,
            params.curves.red,
            params.curves.green,
            params.curves.blue,
        ]
        .map(|c| c.points().iter().map(|p| [p.input, p.output]).collect());
        Self {
            version: 1,
            algorithm: if params.curves.interpolation == CurveInterpolation::Linear {
                1
            } else {
                2
            },
            name,
            temperature: params.temperature,
            tint: params.tint,
            brightness: params.brightness,
            contrast: params.contrast,
            saturation: params.saturation,
            curves,
        }
    }
    fn params(&self) -> Result<AdjustmentParams, String> {
        if self.version != 1 || ![1, 2].contains(&self.algorithm) {
            return Err("不支援此預設集或演算法版本".into());
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err("預設集名稱須為 1–80 字元".into());
        }
        if !(-100..=100).contains(&self.temperature)
            || !(-100..=100).contains(&self.tint)
            || [self.brightness, self.contrast, self.saturation]
                .iter()
                .any(|n| !n.is_finite() || !(0.0..=2.0).contains(n))
        {
            return Err("預設集微調參數超出有效範圍".into());
        }
        let mut curves = [Curve::default(); 4];
        for (out, points) in curves.iter_mut().zip(&self.curves) {
            let points: Vec<_> = points
                .iter()
                .map(|p| CurvePoint {
                    input: p[0],
                    output: p[1],
                })
                .collect();
            *out = Curve::new(&points).map_err(str::to_owned)?;
        }
        Ok(AdjustmentParams {
            temperature: self.temperature,
            tint: self.tint,
            brightness: self.brightness,
            contrast: self.contrast,
            saturation: self.saturation,
            curves: CurveSet {
                interpolation: if self.algorithm == 1 {
                    CurveInterpolation::Linear
                } else {
                    CurveInterpolation::Smooth
                },
                rgb: curves[0],
                red: curves[1],
                green: curves[2],
                blue: curves[3],
            },
        })
    }
}
fn read_preset(path: &Path) -> Result<Preset, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_PRESET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PRESET_BYTES {
        return Err("預設集超過 64 KiB 上限".into());
    }
    let preset: Preset = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    preset.params()?;
    Ok(preset)
}
fn write_preset(path: &Path, bytes: &[u8], overwrite: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.flush())
        .map_err(|e| e.to_string())?;
    if overwrite {
        file.persist(path).map_err(|e| e.to_string())?;
    } else {
        file.persist_noclobber(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
struct Imported {
    identity: (u64, u64),
    previous: AdjustmentParams,
    preset: Preset,
}
enum ResultMessage {
    Imported(Result<Box<Imported>, String>),
    Checked(Result<(PathBuf, Vec<u8>, bool), String>),
    Saved(Result<(), String>),
}
#[derive(Default)]
pub(crate) struct PresetState {
    worker: Option<mpsc::Receiver<ResultMessage>>,
    overwrite: Option<(PathBuf, Vec<u8>)>,
}
impl PresetState {
    pub(crate) fn is_pending(&self) -> bool {
        self.worker.is_some() || self.overwrite.is_some()
    }
}
impl ImageViewerApp {
    pub fn import_toning_preset(&mut self, path: PathBuf) -> bool {
        if self.presets.worker.is_some()
            || self.presets.overwrite.is_some()
            || self.is_document_busy()
            || self.is_loading_image()
            || self.has_pending_leave()
            || self.geometry.is_some()
        {
            return false;
        }
        let Some(session) = &self.session else {
            return false;
        };
        let identity = session.identity();
        let previous = self.slider_params;
        let (tx, rx) = mpsc::sync_channel(1);
        self.presets.worker = Some(rx);
        std::thread::spawn(move || {
            let result = read_preset(&path).map(|preset| {
                Box::new(Imported {
                    identity,
                    previous,
                    preset,
                })
            });
            let _ = tx.send(ResultMessage::Imported(result));
        });
        true
    }
    pub fn export_toning_preset(&mut self, path: PathBuf) -> bool {
        if self.presets.worker.is_some()
            || self.presets.overwrite.is_some()
            || self.has_pending_leave()
        {
            return false;
        }
        if path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("json"))
        {
            self.status_notification =
                Some(("調色預設集請使用 .json 副檔名".into(), Instant::now()));
            return false;
        }
        let Some(session) = &self.session else {
            return false;
        };
        if session.source_path().is_some_and(|p| {
            p.to_string_lossy()
                .eq_ignore_ascii_case(&path.to_string_lossy())
        }) {
            return false;
        }
        let source = session.source_path().map(Path::to_path_buf);
        let name = path.file_stem().map_or_else(
            || "自訂調色".into(),
            |n| n.to_string_lossy().chars().take(80).collect(),
        );
        let preset = Preset::new(name, self.slider_params);
        if preset.params().is_err() {
            return false;
        }
        let Ok(bytes) = serde_json::to_vec_pretty(&preset) else {
            return false;
        };
        let (tx, rx) = mpsc::sync_channel(1);
        self.presets.worker = Some(rx);
        std::thread::spawn(move || {
            // 解析真實路徑，避免相對路徑與符號連結繞過來源圖片保護。
            let same_source = source
                .as_deref()
                .and_then(|p| std::fs::canonicalize(p).ok())
                .zip(std::fs::canonicalize(&path).ok())
                .is_some_and(|(source, target)| source == target);
            if same_source {
                let _ = tx.send(ResultMessage::Checked(Err("預設集不可覆寫來源圖片".into())));
                return;
            }
            let result = path
                .try_exists()
                .map(|exists| (path, bytes, exists))
                .map_err(|e| e.to_string());
            let _ = tx.send(ResultMessage::Checked(result));
        });
        true
    }
    fn save_preset_bytes(&mut self, path: PathBuf, bytes: Vec<u8>, overwrite: bool) {
        let (tx, rx) = mpsc::sync_channel(1);
        self.presets.worker = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(ResultMessage::Saved(write_preset(&path, &bytes, overwrite)));
        });
    }
    pub(crate) fn poll_presets(&mut self) {
        let message = self
            .presets
            .worker
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(message) => Some(message),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(ResultMessage::Saved(Err("預設集背景工作中斷".into())))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            });
        let Some(message) = message else {
            return;
        };
        self.presets.worker = None;
        let result = match message {
            ResultMessage::Imported(Ok(imported)) => {
                if self
                    .session
                    .as_ref()
                    .is_none_or(|s| s.identity() != imported.identity)
                    || self.slider_params != imported.previous
                    || self.geometry.is_some()
                    || self.has_pending_leave()
                    || self.is_document_busy()
                    || self.is_loading_image()
                {
                    Err("圖片或草稿已變更，未套用過期預設集".into())
                } else {
                    match imported.preset.params() {
                        Ok(params) => {
                            self.slider_params = params;
                            self.queue_toning_preview();
                            self.show_side_panel = true;
                            self.convenience.info_tab = false;
                            Ok(format!(
                                "已載入預設集「{}」，請預覽後套用",
                                imported.preset.name
                            ))
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            ResultMessage::Checked(Ok((path, bytes, true))) => {
                self.presets.overwrite = Some((path, bytes));
                return;
            }
            ResultMessage::Checked(Ok((path, bytes, false))) => {
                self.save_preset_bytes(path, bytes, false);
                return;
            }
            ResultMessage::Saved(Ok(())) => Ok("已匯出調色預設集，圖片未變更".into()),
            ResultMessage::Imported(Err(e))
            | ResultMessage::Checked(Err(e))
            | ResultMessage::Saved(Err(e)) => Err(e),
        };
        self.status_notification = Some((
            result.unwrap_or_else(|e| format!("預設集：{e}")),
            Instant::now(),
        ));
    }
    pub(crate) fn queue_toning_preview(&mut self) {
        if let Some(session) = &mut self.session {
            if self.slider_params.is_identity() {
                self.debouncer.cancel_pending();
                self.pending_adjustment = None;
                session.cancel_adjustments();
            } else {
                self.pending_adjustment = Some(self.debouncer.submit(
                    session.committed_image(),
                    Some(session.identity()),
                    crate::debounce_queue::PixelOperation::Adjust(self.slider_params),
                ));
            }
        }
    }
    pub(crate) fn show_preset_dialog(&mut self, ctx: &egui::Context) {
        if self.presets.worker.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        let Some((path, _)) = &self.presets.overwrite else {
            return;
        };
        let mut choice = None;
        egui::Window::new("覆寫調色預設集？")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(path.display().to_string());
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        choice = Some(false);
                    }
                    if ui.button("覆寫").clicked() {
                        choice = Some(true);
                    }
                });
            });
        if let Some(overwrite) = choice {
            let (path, bytes) = self.presets.overwrite.take().unwrap();
            if overwrite {
                self.save_preset_bytes(path, bytes, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    fn curve_params() -> AdjustmentParams {
        AdjustmentParams {
            curves: CurveSet {
                rgb: Curve::new(&[
                    CurvePoint {
                        input: 0,
                        output: 255,
                    },
                    CurvePoint {
                        input: 255,
                        output: 0,
                    },
                ])
                .unwrap(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn app() -> ImageViewerApp {
        let mut app = ImageViewerApp::default();
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::from_pixel(4, 3, image::Rgba([20, 40, 80, 128])),
            None,
        ));
        app
    }
    fn wait(app: &mut ImageViewerApp) {
        let start = Instant::now();
        loop {
            app.poll_presets();
            if let Some(result) = app.debouncer.poll() {
                app.accept_pixel_result(result);
            }
            if app.presets.worker.is_none()
                && app.pending_adjustment.is_none()
                && !app.debouncer.is_computing()
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "背景預設集或預覽未完成"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn preset_roundtrip_preview_apply_undo_redo_and_output_are_consistent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("反相.json");
        let mut app = app();
        app.slider_params = curve_params();
        let source = app.session.as_ref().unwrap().committed_image();
        assert!(app.has_unsaved_changes());
        assert!(app.export_toning_preset(path.clone()));
        wait(&mut app);
        assert_eq!(*app.session.as_ref().unwrap().committed_image(), *source);
        assert!(!app.session.as_ref().unwrap().is_dirty());
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("source_path") && !text.contains("pixels"));
        app.slider_params = Default::default();
        assert!(app.import_toning_preset(path));
        wait(&mut app);
        assert_eq!(app.slider_params, curve_params());
        assert_eq!(
            app.document_status(),
            crate::panels::DocumentStatus::Preview {
                committed_dirty: false,
                computing: false
            }
        );
        assert_eq!(
            app.session
                .as_ref()
                .unwrap()
                .active_image()
                .get_pixel(0, 0)
                .0,
            [235, 215, 175, 128]
        );
        assert_eq!(*app.session.as_ref().unwrap().committed_image(), *source);
        assert!(app.apply_current_adjustments());
        assert_eq!(app.session.as_ref().unwrap().history().undo_len(), 1);
        let output = dir.path().join("結果.png");
        core_image::output::save_with_options(
            &app.session.as_ref().unwrap().committed_image(),
            &output,
            Default::default(),
        )
        .unwrap();
        assert_eq!(
            image::open(output).unwrap().to_rgba8().get_pixel(0, 0).0,
            [235, 215, 175, 128]
        );
        app.undo();
        assert_eq!(*app.session.as_ref().unwrap().committed_image(), *source);
        app.redo();
        assert_eq!(
            app.session
                .as_ref()
                .unwrap()
                .committed_image()
                .get_pixel(0, 0)
                .0,
            [235, 215, 175, 128]
        );
    }
    #[test]
    fn invalid_presets_leave_the_current_draft_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("無效.json");
        let valid = serde_json::to_value(Preset::new("預設".into(), curve_params())).unwrap();
        let mut cases = Vec::new();
        for (key, value) in [
            ("version", serde_json::json!(2)),
            ("algorithm", serde_json::json!(3)),
            ("brightness", serde_json::json!(3)),
            ("name", serde_json::json!("")),
            ("unknown", serde_json::json!(0)),
        ] {
            let mut p = valid.clone();
            p[key] = value;
            cases.push(serde_json::to_vec(&p).unwrap());
        }
        let mut p = valid.clone();
        p["curves"][0] = serde_json::json!([[0, 0], [128, 30], [128, 90], [255, 255]]);
        cases.push(serde_json::to_vec(&p).unwrap());
        cases.push(vec![b' '; MAX_PRESET_BYTES as usize + 1]);
        cases.push(b"{broken".to_vec());
        for bytes in cases {
            std::fs::write(&path, bytes).unwrap();
            let mut app = app();
            app.slider_params.brightness = 1.2;
            let before = app.slider_params;
            assert!(app.import_toning_preset(path.clone()));
            wait(&mut app);
            assert_eq!(app.slider_params, before);
            assert!(!app.session.as_ref().unwrap().has_active_preview());
        }
    }
    #[test]
    fn late_import_does_not_replace_a_new_document_or_changed_draft() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("調色.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&Preset::new("預設".into(), curve_params())).unwrap(),
        )
        .unwrap();
        for changed_document in [false, true] {
            let mut app = app();
            assert!(app.import_toning_preset(path.clone()));
            if changed_document {
                app.session = Some(viewer_state::EditingSession::new(
                    image::RgbaImage::new(2, 2),
                    None,
                ));
            } else {
                app.slider_params.brightness = 1.5;
            }
            let expected = app.slider_params;
            wait(&mut app);
            assert_eq!(app.slider_params, expected);
            assert!(!app.session.as_ref().unwrap().has_active_preview());
        }
    }
    #[test]
    fn overwrite_requires_confirmation_and_failed_replacement_preserves_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("已存在.json");
        std::fs::write(&path, b"original").unwrap();
        assert!(write_preset(&path, b"new", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        let mut app = app();
        assert!(app.export_toning_preset(path.clone()));
        wait(&mut app);
        assert!(app.presets.overwrite.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        app.presets.overwrite = None;
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let _lock = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(&path)
                .unwrap();
            assert!(write_preset(&path, b"new", true).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), b"original");
        }
        write_preset(&path, b"new", true).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
    #[test]
    fn rapid_curve_changes_only_deliver_latest_and_cancel_preserves_base() {
        let mut app = app();
        let source = app.session.as_ref().unwrap().committed_image();
        for output in [200, 100, 60] {
            app.slider_params.curves.rgb = Curve::new(&[
                CurvePoint { input: 0, output },
                CurvePoint {
                    input: 255,
                    output: 255,
                },
            ])
            .unwrap();
            app.queue_toning_preview();
        }
        wait(&mut app);
        assert_eq!(
            *app.session.as_ref().unwrap().active_image(),
            core_image::apply_adjustments_rgba(&source, &app.slider_params)
        );
        assert!(!app.session.as_ref().unwrap().is_committed_dirty());
        app.slider_params = Default::default();
        app.queue_toning_preview();
        wait(&mut app);
        assert!(!app.has_unsaved_changes());
        assert_eq!(*app.session.as_ref().unwrap().committed_image(), *source);
        assert!(!app.session.as_ref().unwrap().can_undo());
    }
    #[test]
    fn preset_algorithm_keeps_legacy_linear_effects_and_new_smooth_effects() {
        let mut params = curve_params();
        params.curves.rgb = Curve::new(&[
            CurvePoint {
                input: 0,
                output: 0,
            },
            CurvePoint {
                input: 128,
                output: 255,
            },
            CurvePoint {
                input: 255,
                output: 255,
            },
        ])
        .unwrap();
        let smooth = Preset::new("平滑".into(), params);
        assert_eq!(smooth.algorithm, 2);
        assert_eq!(smooth.params().unwrap().curves.rgb.lut()[64], 175);
        let mut legacy = smooth;
        legacy.algorithm = 1;
        let loaded = legacy.params().unwrap();
        assert_eq!(loaded.curves.interpolation, CurveInterpolation::Linear);
        assert_eq!(
            loaded.curves.rgb.lut_with(loaded.curves.interpolation)[64],
            128
        );
        assert_eq!(Preset::new("舊預設".into(), loaded).algorithm, 1);
        let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([64, 64, 64, 128]));
        assert_eq!(
            core_image::apply_adjustments_rgba(&pixels, &loaded)
                .get_pixel(0, 0)
                .0,
            [128, 128, 128, 128]
        );
        assert_eq!(
            core_image::apply_adjustments_rgba(&pixels, &params)
                .get_pixel(0, 0)
                .0,
            [175, 175, 175, 128]
        );
    }
    #[test]
    fn preset_export_rejects_an_alias_of_the_source_image() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.json");
        let pixels = image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 40, 80, 255]));
        image::DynamicImage::ImageRgba8(pixels.clone())
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let before = std::fs::read(&source).unwrap();
        let mut app = app();
        app.session = Some(viewer_state::EditingSession::new(pixels, Some(source)));
        let sub = dir.path().join("nested");
        std::fs::create_dir(&sub).unwrap();
        assert!(app.export_toning_preset(sub.join("..").join("source.json")));
        wait(&mut app);
        assert!(
            app.presets.overwrite.is_none(),
            "來源圖片別名不應進入覆寫確認"
        );
        assert_eq!(
            std::fs::read(dir.path().join("source.json")).unwrap(),
            before
        );
    }
    #[test]
    fn close_waits_for_preset_io_and_overwrite_decisions() {
        let mut app = app();
        let (tx, rx) = mpsc::sync_channel(1);
        app.presets.worker = Some(rx);
        assert!(!app.request_close(), "預設集 I/O 未完成時不可直接關閉行程");
        tx.send(ResultMessage::Saved(Ok(()))).unwrap();
        app.poll_presets();
        app.presets.overwrite = Some((PathBuf::from("preset.json"), vec![]));
        assert!(!app.request_close(), "覆寫尚未確認時應保留使用者選擇");
        app.presets.overwrite = None;
        assert!(app.request_close());
    }
}
