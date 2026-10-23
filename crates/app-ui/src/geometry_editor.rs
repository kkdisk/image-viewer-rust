//! 裁切／尺寸草稿與有界背景工作的交付；底圖僅於明確套用後變更。
use crate::{
    app::ImageViewerApp,
    debounce_queue::{AdjustmentResult, PixelOperation},
};
use core_image::geometry::{CropRect, EditOperation, ResizeFilter, proportional_dimension};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GeometryTool {
    Crop,
    Resize,
}
#[derive(Clone, Copy)]
struct Drag {
    origin: egui::Pos2,
    rect: CropRect,
    corner: Option<usize>,
}
pub struct GeometryDraft {
    pub tool: GeometryTool,
    pub identity: (u64, u64),
    pub size: (u32, u32),
    pub crop: CropRect,
    pub width: u32,
    pub height: u32,
    pub ratio: usize,
    pub locked: bool,
    pub percent: f64,
    pub filter: ResizeFilter,
    drag: Option<Drag>,
    pub pending: Option<u64>,
    apply_when_ready: bool,
    error: Option<String>,
    ready: Option<(EditOperation, std::sync::Arc<image::RgbaImage>)>,
}
impl GeometryDraft {
    fn operation(&self) -> EditOperation {
        match self.tool {
            GeometryTool::Crop => EditOperation::Crop(self.crop),
            GeometryTool::Resize => EditOperation::Resize {
                width: self.width,
                height: self.height,
                filter: self.filter,
            },
        }
    }
    fn aspect(&self) -> Option<f32> {
        match self.ratio {
            1 => Some(self.size.0 as f32 / self.size.1 as f32),
            2 => Some(1.0),
            3 => Some(4.0 / 3.0),
            4 => Some(3.0 / 2.0),
            5 => Some(16.0 / 9.0),
            _ => None,
        }
    }
    fn clamp_crop(&mut self) {
        self.crop.x = self.crop.x.min(self.size.0 - 1);
        self.crop.y = self.crop.y.min(self.size.1 - 1);
        self.crop.width = self.crop.width.clamp(1, self.size.0 - self.crop.x);
        self.crop.height = self.crop.height.clamp(1, self.size.1 - self.crop.y);
        if let Some(ratio) = self.aspect() {
            let width = (self.crop.height as f32 * ratio).round().max(1.0) as u32;
            if width <= self.size.0 - self.crop.x {
                self.crop.width = width;
            } else {
                self.crop.width = self.size.0 - self.crop.x;
                self.crop.height = (self.crop.width as f32 / ratio).round().max(1.0) as u32;
            }
            self.crop.height = self.crop.height.min(self.size.1 - self.crop.y);
        }
    }
}
impl ImageViewerApp {
    pub fn set_crop_rect(&mut self, rect: CropRect) -> bool {
        let Some(draft) = &mut self.geometry else {
            return false;
        };
        if draft.tool != GeometryTool::Crop
            || draft.pending.is_some()
            || rect.width == 0
            || rect.height == 0
            || u64::from(rect.x) + u64::from(rect.width) > u64::from(draft.size.0)
            || u64::from(rect.y) + u64::from(rect.height) > u64::from(draft.size.1)
        {
            return false;
        }
        draft.crop = rect;
        draft.clamp_crop();
        true
    }

    pub fn set_resize_dimensions(&mut self, width: u32, height: u32, filter: ResizeFilter) -> bool {
        let Some(draft) = &mut self.geometry else {
            return false;
        };
        if draft.tool != GeometryTool::Resize
            || draft.pending.is_some()
            || width == 0
            || height == 0
            || core_image::check_pixel_limit(width, height, core_image::MAX_SAFE_PIXELS).is_err()
        {
            return false;
        }
        draft.width = width;
        draft.height = height;
        draft.filter = filter;
        true
    }
    /// 僅供測試及非畫面流程等待，正常介面維持每幀非阻塞輪詢。
    pub fn wait_geometry(&mut self, timeout: std::time::Duration) -> bool {
        let start = std::time::Instant::now();
        loop {
            if let Some(result) = self.debouncer.poll() {
                self.accept_pixel_result(result);
            }
            if self.geometry.as_ref().is_none_or(|d| d.pending.is_none())
                && !self.debouncer.is_computing()
            {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    pub fn begin_geometry(&mut self, tool: GeometryTool) -> bool {
        if self.is_document_busy() || self.is_loading_image() || self.has_pending_leave() {
            return false;
        }
        if self.geometry.is_some()
            || self.is_preview_pending()
            || !self.slider_params.is_identity()
            || self
                .session
                .as_ref()
                .is_some_and(|s| s.has_active_preview())
        {
            self.status_notification = Some((
                "請先套用或取消目前草稿，再切換編輯工具".into(),
                std::time::Instant::now(),
            ));
            return false;
        }
        let Some(session) = &mut self.session else {
            return false;
        };
        let (width, height) = session.committed_image().dimensions();
        self.geometry = Some(GeometryDraft {
            tool,
            identity: session.identity(),
            size: (width, height),
            crop: CropRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            width,
            height,
            ratio: 0,
            locked: true,
            percent: 100.0,
            filter: ResizeFilter::Cubic,
            drag: None,
            pending: None,
            apply_when_ready: false,
            error: None,
            ready: None,
        });
        session.begin_geometry_draft();
        self.comparison.is_active = false;
        self.stop_slideshow();
        true
    }
    pub fn cancel_geometry(&mut self) {
        if self.geometry.take().is_some() {
            self.debouncer.cancel_pending();
            if let Some(session) = &mut self.session {
                session.cancel_adjustments();
            }
        }
    }
    pub fn preview_geometry(&mut self) -> bool {
        self.request_geometry(false)
    }
    pub fn apply_geometry(&mut self) -> bool {
        self.request_geometry(true)
    }
    fn request_geometry(&mut self, apply: bool) -> bool {
        let Some(draft) = &mut self.geometry else {
            return false;
        };
        let Some(session) = &mut self.session else {
            return false;
        };
        if draft.identity != session.identity() || draft.pending.is_some() {
            return false;
        }
        let operation = draft.operation();
        if apply
            && let Some((ready_op, image)) = &draft.ready
            && *ready_op == operation
        {
            session.set_geometry_preview(image.clone());
            session.apply_geometry_preview(operation.label());
            self.geometry = None;
            self.viewport.reset_fit_window();
            return true;
        }
        if !apply && draft.tool != GeometryTool::Resize {
            return false;
        }
        draft.error = None;
        draft.apply_when_ready = apply;
        draft.pending = Some(self.debouncer.submit(
            session.committed_image(),
            Some(draft.identity),
            PixelOperation::Geometry(operation),
        ));
        true
    }
    pub(crate) fn accept_pixel_result(&mut self, result: AdjustmentResult) {
        if result.identity.is_some_and(|identity| {
            self.session
                .as_ref()
                .is_none_or(|s| s.identity() != identity)
        }) {
            return;
        }
        match result.operation {
            PixelOperation::Adjust(params) => {
                self.pending_adjustment = None;
                if let (Some(session), Ok(image)) = (&mut self.session, result.pixels) {
                    session.set_preview_image(image, params);
                }
            }
            PixelOperation::Geometry(op) => {
                let Some(draft) = &mut self.geometry else {
                    return;
                };
                if draft.pending != Some(result.request_id) || draft.operation() != op {
                    return;
                }
                draft.pending = None;
                match result.pixels {
                    Ok(image) if draft.apply_when_ready => {
                        if let Some(session) = &mut self.session {
                            session.set_geometry_preview(image);
                            session.apply_geometry_preview(op.label());
                        }
                        self.geometry = None;
                        self.viewport.reset_fit_window();
                    }
                    Ok(image) => {
                        if let Some(session) = &mut self.session {
                            session.set_geometry_preview(image.clone());
                        }
                        draft.ready = Some((op, image));
                        self.viewport.reset_fit_window();
                    }
                    Err(error) => {
                        draft.error = Some(error);
                        draft.apply_when_ready = false;
                    }
                }
            }
        }
    }
    pub(crate) fn show_geometry_editor(&mut self, ctx: &egui::Context) {
        let Some(draft) = &mut self.geometry else {
            return;
        };
        let mut cancel = false;
        let mut apply = false;
        let mut preview = false;
        egui::TopBottomPanel::top("geometry_editor").show(ctx, |ui| {
            ui.heading(if draft.tool == GeometryTool::Crop {
                "裁切圖片"
            } else {
                "變更圖片尺寸"
            });
            ui.add_enabled_ui(draft.pending.is_none(), |ui| {
                if draft.tool == GeometryTool::Crop {
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("crop_ratio")
                            .selected_text(
                                ["自由", "原比例", "1:1", "4:3", "3:2", "16:9"][draft.ratio],
                            )
                            .show_ui(ui, |ui| {
                                for (i, label) in ["自由", "原比例", "1:1", "4:3", "3:2", "16:9"]
                                    .iter()
                                    .enumerate()
                                {
                                    ui.selectable_value(&mut draft.ratio, i, *label);
                                }
                            });
                        ui.label("X");
                        ui.add(egui::DragValue::new(&mut draft.crop.x));
                        ui.label("Y");
                        ui.add(egui::DragValue::new(&mut draft.crop.y));
                        ui.label("寬");
                        let w = ui
                            .add(egui::DragValue::new(&mut draft.crop.width))
                            .changed();
                        ui.label("高");
                        ui.add(egui::DragValue::new(&mut draft.crop.height));
                        if w && let Some(ratio) = draft.aspect() {
                            draft.crop.height =
                                (draft.crop.width as f32 / ratio).round().max(1.0) as u32;
                        }
                    });
                    draft.clamp_crop();
                    ui.label("拖曳框內移動、拖曳四角調整；數值以完整影像像素計算。");
                } else {
                    ui.horizontal(|ui| {
                        ui.label("寬");
                        let w = ui
                            .add(egui::DragValue::new(&mut draft.width).range(1..=u32::MAX))
                            .changed();
                        ui.label("高");
                        let h = ui
                            .add(egui::DragValue::new(&mut draft.height).range(1..=u32::MAX))
                            .changed();
                        ui.checkbox(&mut draft.locked, "鎖定比例");
                        if draft.locked {
                            if w {
                                draft.height =
                                    proportional_dimension(draft.width, draft.size.1, draft.size.0)
                                        .unwrap_or(u32::MAX);
                            } else if h {
                                draft.width = proportional_dimension(
                                    draft.height,
                                    draft.size.0,
                                    draft.size.1,
                                )
                                .unwrap_or(u32::MAX);
                            }
                        }
                        if ui
                            .add(
                                egui::DragValue::new(&mut draft.percent)
                                    .range(0.01..=10000.0)
                                    .suffix("% 原尺寸"),
                            )
                            .changed()
                        {
                            draft.width = (draft.size.0 as f64 * draft.percent / 100.0)
                                .round()
                                .clamp(1.0, u32::MAX as f64)
                                as u32;
                            draft.height = (draft.size.1 as f64 * draft.percent / 100.0)
                                .round()
                                .clamp(1.0, u32::MAX as f64)
                                as u32;
                        }
                        ui.selectable_value(&mut draft.filter, ResizeFilter::Cubic, "雙三次");
                        ui.selectable_value(&mut draft.filter, ResizeFilter::Pixel, "最近鄰");
                    });
                    ui.label(format!(
                        "原尺寸 {} × {} → {} × {}；套用後可復原",
                        draft.size.0, draft.size.1, draft.width, draft.height
                    ));
                }
                ui.horizontal(|ui| {
                    if draft.tool == GeometryTool::Resize {
                        preview = ui.button("預覽").clicked();
                    }
                    apply = ui.button("套用").clicked();
                    cancel = ui.button("取消").clicked();
                });
            });
            if draft.pending.is_some() {
                ui.spinner();
                ui.label("正在背景產生完整像素…");
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
            if let Some(error) = &draft.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
        });
        if draft
            .ready
            .as_ref()
            .is_some_and(|(op, _)| *op != draft.operation())
        {
            draft.ready = None;
            if let Some(session) = &mut self.session {
                session.cancel_adjustments();
                session.begin_geometry_draft();
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if cancel {
            self.cancel_geometry();
        } else if apply {
            self.apply_geometry();
        } else if preview {
            self.preview_geometry();
        }
    }
    pub(crate) fn draw_crop_overlay(&mut self, ui: &mut egui::Ui, image_rect: Option<egui::Rect>) {
        let Some(draft) = &mut self.geometry else {
            return;
        };
        if draft.tool != GeometryTool::Crop || draft.pending.is_some() {
            return;
        }
        let Some(image_rect) = image_rect else {
            return;
        };
        let scale = image_rect.size() / egui::vec2(draft.size.0 as f32, draft.size.1 as f32);
        let r = draft.crop;
        let selected = egui::Rect::from_min_size(
            image_rect.min + egui::vec2(r.x as f32, r.y as f32) * scale,
            egui::vec2(r.width as f32, r.height as f32) * scale,
        );
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(image_rect));
        for rect in [
            egui::Rect::from_min_max(image_rect.min, egui::pos2(image_rect.max.x, selected.min.y)),
            egui::Rect::from_min_max(egui::pos2(image_rect.min.x, selected.max.y), image_rect.max),
            egui::Rect::from_min_max(
                egui::pos2(image_rect.min.x, selected.min.y),
                egui::pos2(selected.min.x, selected.max.y),
            ),
            egui::Rect::from_min_max(
                egui::pos2(selected.max.x, selected.min.y),
                egui::pos2(image_rect.max.x, selected.max.y),
            ),
        ] {
            painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(140));
        }
        painter.rect_stroke(
            selected,
            0.0,
            egui::Stroke::new(2.0_f32, egui::Color32::WHITE),
        );
        let corners = [
            selected.left_top(),
            selected.right_top(),
            selected.right_bottom(),
            selected.left_bottom(),
        ];
        for pos in corners {
            painter.rect_filled(
                egui::Rect::from_center_size(pos, egui::vec2(8.0, 8.0)),
                1.0,
                egui::Color32::WHITE,
            );
        }
        let response = ui.interact(
            image_rect.intersect(ui.clip_rect()),
            egui::Id::new("crop_drag"),
            egui::Sense::drag(),
        );
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let origin = ui
                .input(|input| input.pointer.press_origin())
                .unwrap_or(pos);
            let corner = corners.iter().position(|p| p.distance(origin) <= 14.0);
            draft.drag = Some(Drag {
                origin,
                rect: r,
                corner: corner.or_else(|| {
                    if selected.contains(origin) {
                        None
                    } else {
                        Some(2)
                    }
                }),
            });
        }

        if response.dragged()
            && let (Some(drag), Some(pos)) = (draft.drag, response.interact_pointer_pos())
        {
            let delta = (pos - drag.origin) / scale;
            let r = drag.rect;
            if let Some(corner) = drag.corner {
                let mut min = egui::pos2(r.x as f32, r.y as f32);
                let mut max = egui::pos2((r.x + r.width) as f32, (r.y + r.height) as f32);
                if corner == 0 || corner == 3 {
                    min.x += delta.x;
                } else {
                    max.x += delta.x;
                }
                if corner == 0 || corner == 1 {
                    min.y += delta.y;
                } else {
                    max.y += delta.y;
                }
                let x = min
                    .x
                    .min(max.x)
                    .floor()
                    .clamp(0.0, (draft.size.0 - 1) as f32) as u32;
                let y = min
                    .y
                    .min(max.y)
                    .floor()
                    .clamp(0.0, (draft.size.1 - 1) as f32) as u32;
                let right = min
                    .x
                    .max(max.x)
                    .ceil()
                    .clamp((x + 1) as f32, draft.size.0 as f32) as u32;
                let bottom = min
                    .y
                    .max(max.y)
                    .ceil()
                    .clamp((y + 1) as f32, draft.size.1 as f32) as u32;
                draft.crop = CropRect {
                    x,
                    y,
                    width: right - x,
                    height: bottom - y,
                };
            } else {
                draft.crop.x = (r.x as f32 + delta.x)
                    .round()
                    .clamp(0.0, (draft.size.0 - r.width) as f32)
                    as u32;
                draft.crop.y = (r.y as f32 + delta.y)
                    .round()
                    .clamp(0.0, (draft.size.1 - r.height) as f32)
                    as u32;
            }
            draft.clamp_crop();
        }
        if response.drag_stopped() {
            draft.drag = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        app: &mut ImageViewerApp,
        ctx: &egui::Context,
        rect: egui::Rect,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.draw_crop_overlay(ui, Some(rect)));
            },
        );
    }
    #[test]
    fn corner_drag_maps_full_pixels_at_multiple_scales_and_dpi() {
        for scale in [10.0, 25.0, 40.0] {
            for density in [1.0, 2.0] {
                let mut app = ImageViewerApp::default();
                app.session = Some(viewer_state::EditingSession::new(
                    image::RgbaImage::new(12, 8),
                    None,
                ));
                assert!(app.begin_geometry(GeometryTool::Crop));
                let ctx = egui::Context::default();
                ctx.set_pixels_per_point(density);
                let rect = egui::Rect::from_min_size(
                    egui::pos2(20.0, 40.0),
                    egui::vec2(12.0 * scale, 8.0 * scale),
                );
                frame(&mut app, &ctx, rect, vec![]);
                let origin = rect.max - egui::vec2(1.0, 1.0);
                frame(
                    &mut app,
                    &ctx,
                    rect,
                    vec![
                        egui::Event::PointerMoved(origin),
                        egui::Event::PointerButton {
                            pos: origin,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        },
                    ],
                );
                let target = origin - egui::vec2(6.0 * scale, 4.0 * scale);
                frame(
                    &mut app,
                    &ctx,
                    rect,
                    vec![egui::Event::PointerMoved(target)],
                );
                frame(
                    &mut app,
                    &ctx,
                    rect,
                    vec![egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                );
                assert_eq!(
                    app.geometry.as_ref().unwrap().crop,
                    CropRect {
                        x: 0,
                        y: 0,
                        width: 6,
                        height: 4
                    }
                );
            }
        }
    }
}
