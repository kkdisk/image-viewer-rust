//! 瀏覽模式與同源快捷鍵定義；說明直接使用實際派送表。
use crate::app::ImageViewerApp;
use egui::Key;
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct BrowsingState {
    pub fullscreen: bool,
    pub help: bool,
    last_pointer: Option<egui::Pos2>,
    moved: Option<Instant>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Refresh,
    Delete,
    Slideshow,
    CopyPath,
    Open,
    Save,
    SaveAs,
    Undo,
    Redo,
    Compare,
    Prev,
    Next,
    First,
    Last,
    Fit,
    Actual,
    In,
    Out,
    Magnifier,
    MagnifierZoom,
    Fullscreen,
    Help,
}
struct Binding {
    keys: &'static [Key],
    command: bool,
    shift: Option<bool>,
    label: &'static str,
    description: &'static str,
    action: Action,
}
macro_rules! binding {
    ($keys:expr, $command:expr, $shift:expr, $label:expr, $description:expr, $action:ident) => {
        Binding {
            keys: $keys,
            command: $command,
            shift: $shift,
            label: $label,
            description: $description,
            action: Action::$action,
        }
    };
}
const BINDINGS: &[Binding] = &[
    binding!(
        &[Key::F5],
        false,
        Some(false),
        "F5",
        "重新整理並重載",
        Refresh
    ),
    binding!(
        &[Key::Delete],
        false,
        Some(false),
        "Delete",
        "移至回收桶",
        Delete
    ),
    binding!(
        &[Key::Space],
        false,
        Some(false),
        "空白鍵",
        "開始／停止幻燈片",
        Slideshow
    ),
    binding!(
        &[Key::C],
        true,
        Some(true),
        "Ctrl+Shift+C",
        "複製來源路徑",
        CopyPath
    ),
    binding!(
        &[Key::O],
        true,
        Some(false),
        "Ctrl+O",
        "開啟圖片或壓縮檔",
        Open
    ),
    binding!(
        &[Key::S],
        true,
        Some(false),
        "Ctrl+S",
        "儲存已套用底圖",
        Save
    ),
    binding!(
        &[Key::S],
        true,
        Some(true),
        "Ctrl+Shift+S",
        "另存新檔",
        SaveAs
    ),
    binding!(&[Key::Z], true, Some(false), "Ctrl+Z", "復原", Undo),
    binding!(&[Key::Z], true, Some(true), "Ctrl+Shift+Z", "重做", Redo),
    binding!(&[Key::Y], true, Some(false), "Ctrl+Y", "重做", Redo),
    binding!(&[Key::D], true, Some(false), "Ctrl+D", "雙圖比較", Compare),
    binding!(
        &[Key::ArrowLeft, Key::PageUp],
        false,
        Some(false),
        "←／PageUp",
        "上一張",
        Prev
    ),
    binding!(
        &[Key::ArrowRight, Key::PageDown],
        false,
        Some(false),
        "→／PageDown",
        "下一張",
        Next
    ),
    binding!(&[Key::Home], false, Some(false), "Home", "第一張", First),
    binding!(&[Key::End], false, Some(false), "End", "最後一張", Last),
    binding!(
        &[Key::F, Key::Num0],
        false,
        Some(false),
        "F／0",
        "符合視窗",
        Fit
    ),
    binding!(&[Key::Num1], false, Some(false), "1", "原始尺寸", Actual),
    binding!(&[Key::Equals, Key::Plus], false, None, "＋／＝", "放大", In),
    binding!(&[Key::Minus], false, Some(false), "−", "縮小", Out),
    binding!(&[Key::M], false, Some(false), "M", "放大鏡", Magnifier),
    binding!(
        &[Key::M],
        false,
        Some(true),
        "Shift+M",
        "切換放大鏡倍率",
        MagnifierZoom
    ),
    binding!(
        &[Key::F11],
        false,
        Some(false),
        "F11",
        "切換全螢幕",
        Fullscreen
    ),
    binding!(&[Key::F1], false, Some(false), "F1", "快捷鍵說明", Help),
];
pub(crate) fn actions(ctx: &egui::Context) -> Vec<Action> {
    ctx.input(|input| {
        let mut result = Vec::new();
        for binding in BINDINGS {
            if input.modifiers.command == binding.command
                && !input.modifiers.alt
                && binding
                    .shift
                    .is_none_or(|shift| input.modifiers.shift == shift)
                && binding.keys.iter().any(|&key| input.key_pressed(key))
                && !result.contains(&binding.action)
            {
                result.push(binding.action);
            }
        }
        result
    })
}
impl ImageViewerApp {
    pub fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        if self.geometry.is_some() {
            self.status_notification = Some((
                "請先套用或取消編輯草稿，再切換全螢幕".into(),
                Instant::now(),
            ));
            return;
        }
        self.stop_slideshow();
        self.browsing.fullscreen = !self.browsing.fullscreen;
        self.browsing.moved = Some(Instant::now());
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.browsing.fullscreen));
    }
    pub(crate) fn show_browsing_overlay(&mut self, ctx: &egui::Context) {
        if self.browsing.fullscreen {
            let pointer = ctx.input(|input| input.pointer.hover_pos());
            if pointer != self.browsing.last_pointer {
                self.browsing.last_pointer = pointer;
                self.browsing.moved = Some(Instant::now());
            }
            if let Some(moved) = self.browsing.moved
                && moved.elapsed() < Duration::from_secs(2)
            {
                egui::Area::new(egui::Id::new("fullscreen_hud"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(ctx.screen_rect().min + egui::vec2(16.0, 16.0))
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            let name = self
                                .session
                                .as_ref()
                                .and_then(|session| session.source_path())
                                .and_then(|path| path.file_name())
                                .map_or_else(|| "未載入圖片".into(), |name| name.to_string_lossy());
                            ui.label(format!(
                                "{}　{} / {}　{:.0}%",
                                name,
                                self.playlist.current_index().map_or(0, |index| index + 1),
                                self.playlist.len(),
                                self.viewport.zoom * 100.0
                            ));
                            if self.slideshow.playing {
                                ui.label(format!(
                                    "幻燈片播放中；略過 {} 張載入失敗圖片（空白鍵停止）",
                                    self.slideshow.failures
                                ));
                            }
                            if ui.button("離開全螢幕（F11／Esc）").clicked() {
                                self.toggle_fullscreen(ctx);
                            }
                        });
                    });
                ctx.request_repaint_after(Duration::from_secs(2).saturating_sub(moved.elapsed()));
            }
        }
        egui::Window::new("快捷鍵說明")
            .open(&mut self.browsing.help)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("shortcuts").striped(true).show(ui, |ui| {
                    for binding in BINDINGS {
                        ui.label(binding.label);
                        ui.label(binding.description);
                        ui.end_row();
                    }
                    ui.label("Esc");
                    ui.label("先關閉對話框，再離開全螢幕");
                    ui.end_row();
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contains(shape: &egui::Shape, text: &str) -> bool {
        match shape {
            egui::Shape::Text(shape) => shape.galley.text() == text,
            egui::Shape::Vec(shapes) => shapes.iter().any(|shape| contains(shape, text)),
            _ => false,
        }
    }
    #[test]
    fn fullscreen_hud_expires_without_changing_panel_preferences() {
        let ctx = egui::Context::default();
        let mut app = ImageViewerApp::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 800.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.toggle_fullscreen(ctx);
                app.show_browsing_overlay(ctx);
            },
        );
        assert!(output.viewport_output.values().any(|viewport| {
            viewport
                .commands
                .contains(&egui::ViewportCommand::Fullscreen(true))
        }));
        app.browsing.moved = Some(Instant::now() - Duration::from_secs(3));
        let output = ctx.run(Default::default(), |ctx| app.show_browsing_overlay(ctx));
        assert!(
            !output
                .shapes
                .iter()
                .any(|shape| contains(&shape.shape, "離開全螢幕（F11／Esc）"))
        );
        assert!(app.show_side_panel && app.show_filmstrip);
    }
    #[test]
    fn shortcut_dispatch_matches_the_help_bindings() {
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::F1,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
                ..Default::default()
            },
            |ctx| {
                assert!(actions(ctx).contains(&Action::Help));
            },
        );
        assert!(
            BINDINGS
                .iter()
                .any(|binding| binding.action == Action::Help && binding.label == "F1")
        );
    }
}
