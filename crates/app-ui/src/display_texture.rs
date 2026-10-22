//! 顯示紋理背景轉換；只保留最新等待快照，交付時再次核對影像及裝置上限。

use crate::preferences::SamplingMode;
use image::RgbaImage;
use std::sync::{Arc, Mutex, mpsc};

#[derive(Clone, Copy, PartialEq, Eq)]
struct DisplayKey {
    crop: [u32; 4],
    limit: u32,
    sampling: SamplingMode,
    target: Option<[u32; 2]>,
}
#[derive(Clone, Copy, PartialEq)]
struct ViewSignature {
    zoom: f32,
    pan: (f32, f32),
    auto_fit: bool,
    rect: egui::Rect,
    density: f32,
    sampling: SamplingMode,
}
type Request = (Arc<RgbaImage>, DisplayKey);
struct ResultImage {
    source: Arc<RgbaImage>,
    key: DisplayKey,
    color: egui::ColorImage,
}

pub(crate) struct DisplayTexture {
    pending: Arc<Mutex<Option<Request>>>,
    wake: mpsc::SyncSender<()>,
    results: mpsc::Receiver<ResultImage>,
    requested: Option<Request>,
    ready: bool,
    sampling: SamplingMode,
    observed: Option<(Arc<RgbaImage>, ViewSignature)>,
    changed_at: f64,
    pub region: egui::Rect,
}

impl Default for DisplayTexture {
    fn default() -> Self {
        let pending = Arc::new(Mutex::new(None::<Request>));
        let slot = Arc::clone(&pending);
        let (wake, tasks) = mpsc::sync_channel(1);
        let (sender, results) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while tasks.recv().is_ok() {
                let request = match slot.lock() {
                    Ok(mut slot) => slot.take(),
                    Err(_) => break,
                };
                if let Some((source, key)) = request {
                    let [x, y, width, height] = key.crop;
                    let crop = image::imageops::crop_imm(source.as_ref(), x, y, width, height);
                    let pixels = match (key.sampling, key.target) {
                        (SamplingMode::HighQuality, Some(size)) => {
                            core_image::display_preview::high_quality_preview(
                                &*crop, size, key.limit,
                            )
                        }
                        (SamplingMode::Pixel, _) => {
                            let [w, h] = core_image::display_preview::bounded_display_size(
                                [width, height],
                                key.limit,
                            );
                            image::imageops::resize(
                                &*crop,
                                w,
                                h,
                                image::imageops::FilterType::Nearest,
                            )
                        }
                        _ => core_image::display_preview::display_preview(&*crop, key.limit),
                    };
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [pixels.width() as usize, pixels.height() as usize],
                        pixels.as_raw(),
                    );
                    if sender.send(ResultImage { source, key, color }).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            pending,
            wake,
            results,
            requested: None,
            ready: false,
            sampling: SamplingMode::default(),
            observed: None,
            changed_at: 0.0,
            region: egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        }
    }
}

impl DisplayTexture {
    pub fn set_sampling(&mut self, sampling: SamplingMode) {
        self.sampling = sampling;
    }
    #[cfg(test)]
    pub fn sync(
        &mut self,
        ctx: &egui::Context,
        source: Arc<RgbaImage>,
        texture: &mut Option<egui::TextureHandle>,
        name: &str,
    ) {
        let limit = ctx
            .input(|input| input.max_texture_side)
            .max(1)
            .min(u32::MAX as usize) as u32;
        let key = DisplayKey {
            crop: [0, 0, source.width(), source.height()],
            limit,
            sampling: SamplingMode::Fast,
            target: None,
        };
        self.deliver(ctx, source, texture, name, key);
    }

    pub fn sync_view(
        &mut self,
        ctx: &egui::Context,
        source: Arc<RgbaImage>,
        texture: &mut Option<egui::TextureHandle>,
        name: &str,
        view: &viewer_state::ViewportState,
        canvas_id: &str,
    ) {
        let gpu = ctx
            .input(|input| input.max_texture_side)
            .max(1)
            .min(u32::MAX as usize) as u32;
        let rect =
            ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new(canvas_id).with("bounds")));
        let rect = rect.unwrap_or_else(|| ctx.available_rect());
        let density = ctx.pixels_per_point();
        let now = ctx.input(|input| input.time);
        let signature = ViewSignature {
            zoom: view.zoom,
            pan: view.pan,
            auto_fit: view.auto_fit,
            rect,
            density,
            sampling: self.sampling,
        };
        if !self
            .observed
            .as_ref()
            .is_some_and(|(image, old)| Arc::ptr_eq(image, &source) && *old == signature)
        {
            self.observed = Some((Arc::clone(&source), signature));
            self.changed_at = now;
        }
        let mut key = display_key(source.dimensions(), view, rect, density, gpu);
        let interacting = ctx
            .input(|input| input.pointer.any_down() || input.raw_scroll_delta != egui::Vec2::ZERO);
        if interacting {
            self.changed_at = now;
        }
        if self.sampling == SamplingMode::Pixel {
            key.sampling = SamplingMode::Pixel;
        } else if self.sampling == SamplingMode::HighQuality {
            let remaining = (0.150 - (now - self.changed_at)).max(0.0);
            if remaining == 0.0 && !interacting {
                key = high_quality_key(key, source.dimensions(), view, rect, density);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(remaining.max(0.016)));
            }
        }
        self.deliver(ctx, source, texture, name, key);
    }

    fn deliver(
        &mut self,
        ctx: &egui::Context,
        source: Arc<RgbaImage>,
        texture: &mut Option<egui::TextureHandle>,
        name: &str,
        key: DisplayKey,
    ) {
        let changed = !self
            .requested
            .as_ref()
            .is_some_and(|(image, old_key)| Arc::ptr_eq(image, &source) && *old_key == key);
        if changed {
            if !self
                .requested
                .as_ref()
                .is_some_and(|(image, _)| Arc::ptr_eq(image, &source))
            {
                *texture = None;
            }
            self.requested = Some((Arc::clone(&source), key));
            self.ready = false;
            if let Ok(mut pending) = self.pending.lock() {
                *pending = Some((Arc::clone(&source), key));
            }
            let _ = self.wake.try_send(());
        }
        while let Ok(result) = self.results.try_recv() {
            if Arc::ptr_eq(&result.source, &source) && result.key == key {
                let options = if key.sampling == SamplingMode::Pixel {
                    egui::TextureOptions::NEAREST
                } else {
                    egui::TextureOptions::LINEAR
                };
                *texture = Some(ctx.load_texture(name, result.color, options));
                let [x, y, width, height] = key.crop;
                self.region = egui::Rect::from_min_max(
                    egui::pos2(
                        x as f32 / source.width() as f32,
                        y as f32 / source.height() as f32,
                    ),
                    egui::pos2(
                        (x + width) as f32 / source.width() as f32,
                        (y + height) as f32 / source.height() as f32,
                    ),
                );
                self.ready = true;
            }
        }
        if !self.ready {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    pub fn clear(&mut self) {
        self.observed = None;
        self.requested = None;
        self.ready = false;
        if let Ok(mut pending) = self.pending.lock() {
            *pending = None;
        }
        while self.results.try_recv().is_ok() {}
    }
}

fn display_key(
    (width, height): (u32, u32),
    view: &viewer_state::ViewportState,
    rect: egui::Rect,
    density: f32,
    gpu: u32,
) -> DisplayKey {
    let zoom = if view.auto_fit {
        ((rect.width() - 20.0).max(10.0) / width as f32)
            .min((rect.height() - 20.0).max(10.0) / height as f32)
    } else {
        view.zoom.max(0.0001)
    };
    let pan = if view.auto_fit {
        egui::Vec2::ZERO
    } else {
        egui::vec2(view.pan.0, view.pan.1)
    };
    let min = rect.center() + pan - egui::vec2(width as f32, height as f32) * zoom / 2.0;
    let start = (rect.min - min) / zoom;
    let end = (rect.max - min) / zoom;
    let x = ((start.x.max(0.0) as u32 / 128) * 128).min(width.saturating_sub(1));
    let y = ((start.y.max(0.0) as u32 / 128) * 128).min(height.saturating_sub(1));
    let right = (((end.x.max(0.0) as u32 / 128) + 2).saturating_mul(128)).clamp(x + 1, width);
    let bottom = (((end.y.max(0.0) as u32 / 128) + 2).saturating_mul(128)).clamp(y + 1, height);
    let needed =
        (((right - x).max(bottom - y) as f32 * zoom * density).ceil() as u32).clamp(1, 4096);
    let limit = needed.next_power_of_two().min(gpu).clamp(1, 4096);
    DisplayKey {
        crop: [x, y, right - x, bottom - y],
        limit,
        sampling: SamplingMode::Fast,
        target: None,
    }
}

fn high_quality_key(
    mut key: DisplayKey,
    (width, height): (u32, u32),
    view: &viewer_state::ViewportState,
    rect: egui::Rect,
    density: f32,
) -> DisplayKey {
    let scale = if view.auto_fit {
        ((rect.width() - 20.0).max(10.0) / width as f32)
            .min((rect.height() - 20.0).max(10.0) / height as f32)
    } else {
        view.zoom
    } * density;
    if scale > 1.0 {
        let size = core_image::display_preview::bounded_display_size(
            [
                (key.crop[2] as f32 * scale).ceil() as u32,
                (key.crop[3] as f32 * scale).ceil() as u32,
            ],
            key.limit,
        );
        if size[0] > key.crop[2] || size[1] > key.crop[3] {
            key.sampling = SamplingMode::HighQuality;
            key.target = Some(size);
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn frame(
        display: &mut DisplayTexture,
        ctx: &egui::Context,
        source: &Arc<RgbaImage>,
        texture: &mut Option<egui::TextureHandle>,
        view: &viewer_state::ViewportState,
        time: f64,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                time: Some(time),
                max_texture_side: Some(1024),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 240.0),
                )),
                ..Default::default()
            },
            |ctx| display.sync_view(ctx, Arc::clone(source), texture, "quality", view, "quality"),
        );
    }
    #[test]
    fn idle_250_percent_uses_cubic_texture_and_pan_resets_timer() {
        let ctx = egui::Context::default();
        let source = Arc::new(RgbaImage::from_pixel(
            64,
            32,
            image::Rgba([12, 34, 56, 255]),
        ));
        let mut display = DisplayTexture::default();
        let mut texture = None;
        let mut view = viewer_state::ViewportState {
            zoom: 2.5,
            auto_fit: false,
            ..Default::default()
        };
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.0);
        assert!(display.requested.as_ref().unwrap().1.target.is_none());
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.149);
        assert!(display.requested.as_ref().unwrap().1.target.is_none());
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.151);
        assert_eq!(
            display.requested.as_ref().unwrap().1.target,
            Some([160, 80])
        );
        let start = Instant::now();
        while !display.ready {
            frame(&mut display, &ctx, &source, &mut texture, &view, 0.2);
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(texture.as_ref().unwrap().size(), [160, 80]);
        view.pan.0 = 1.0;
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.21);
        assert!(display.requested.as_ref().unwrap().1.target.is_none());
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.35);
        assert!(display.requested.as_ref().unwrap().1.target.is_none());
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.37);
        assert!(display.requested.as_ref().unwrap().1.target.is_some());
    }
    #[test]
    fn quality_dimensions_include_dpi_and_stay_within_gpu_budget() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 240.0));
        let mut view = viewer_state::ViewportState {
            zoom: 2.5,
            auto_fit: false,
            ..Default::default()
        };
        let key = display_key((64, 32), &view, rect, 1.5, 8192);
        assert_eq!(
            high_quality_key(key, (64, 32), &view, rect, 1.5).target,
            Some([240, 120])
        );
        for gpu in [1, 32, 256, 16384] {
            view.zoom = 50.0;
            let key = display_key((12000, 8000), &view, rect, 2.0, gpu);
            let quality = high_quality_key(key, (12000, 8000), &view, rect, 2.0);
            if let Some([w, h]) = quality.target {
                assert!(w.max(h) <= gpu.min(4096));
                assert!(u64::from(w) * u64::from(h) * 4 <= 64 * 1024 * 1024);
            }
        }
        view.zoom = 1.0;
        let key = display_key((64, 32), &view, rect, 1.0, 8192);
        assert!(
            high_quality_key(key, (64, 32), &view, rect, 1.0)
                .target
                .is_none()
        );
    }
    #[test]
    fn stale_quality_cannot_override_pixel_mode_or_another_source() {
        let ctx = egui::Context::default();
        let source = Arc::new(RgbaImage::new(4, 4));
        let view = viewer_state::ViewportState {
            zoom: 2.5,
            auto_fit: false,
            ..Default::default()
        };
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 240.0));
        let key = high_quality_key(
            display_key((4, 4), &view, rect, 1.0, 1024),
            (4, 4),
            &view,
            rect,
            1.0,
        );
        let (tx, rx) = mpsc::channel();
        let mut display = DisplayTexture {
            results: rx,
            sampling: SamplingMode::Pixel,
            ..Default::default()
        };
        tx.send(ResultImage {
            source: Arc::clone(&source),
            key,
            color: egui::ColorImage::new([10, 10], egui::Color32::RED),
        })
        .unwrap();
        let mut texture = None;
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.0);
        assert!(texture.is_none());
        assert_eq!(
            display.requested.as_ref().unwrap().1.sampling,
            SamplingMode::Pixel
        );
        tx.send(ResultImage {
            source: Arc::new(RgbaImage::new(4, 4)),
            key: display.requested.as_ref().unwrap().1,
            color: egui::ColorImage::new([4, 4], egui::Color32::RED),
        })
        .unwrap();
        frame(&mut display, &ctx, &source, &mut texture, &view, 0.2);
        assert!(texture.is_none());
    }
    #[test]
    fn pixel_mode_uses_nearest_sampler_and_fast_mode_does_not_upscale() {
        for sampling in [SamplingMode::Fast, SamplingMode::Pixel] {
            let ctx = egui::Context::default();
            let source = Arc::new(RgbaImage::new(64, 32));
            let mut display = DisplayTexture {
                sampling,
                ..Default::default()
            };
            let mut texture = None;
            let view = viewer_state::ViewportState {
                zoom: 4.0,
                auto_fit: false,
                ..Default::default()
            };
            let start = Instant::now();
            while !display.ready {
                frame(&mut display, &ctx, &source, &mut texture, &view, 0.5);
                assert!(start.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(1));
            }
            let texture = texture.unwrap();
            assert_eq!(texture.size(), [64, 32]);
            let options = ctx.tex_manager().read().meta(texture.id()).unwrap().options;
            assert_eq!(
                options,
                if sampling == SamplingMode::Pixel {
                    egui::TextureOptions::NEAREST
                } else {
                    egui::TextureOptions::LINEAR
                }
            );
        }
    }

    #[test]
    fn viewport_preview_preserves_native_detail_and_bounds_memory() {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 600.0));
        let mut view = viewer_state::ViewportState::new();
        let fit = display_key((12000, 8000), &view, rect, 1.0, 16384);
        assert!(fit.limit <= 2048);
        view.auto_fit = false;
        view.zoom = 1.0;
        let zoomed = display_key((12000, 8000), &view, rect, 1.0, 16384);
        assert!(zoomed.crop[2] < 1600 && zoomed.crop[3] < 1200);
        assert!(zoomed.limit >= zoomed.crop[2].max(zoomed.crop[3]));
        view.pan = (f32::MAX, -f32::MAX);
        let edge = display_key((12000, 8000), &view, rect, 2.0, 512);
        assert!(edge.crop[0] + edge.crop[2] <= 12000 && edge.crop[1] + edge.crop[3] <= 8000);
        assert!(edge.limit <= 512);
    }

    #[test]
    #[ignore = "固定測資量測，需另行記錄硬體與程序工作集"]
    fn resource_baseline() {
        for (width, height) in [(1920, 1080), (6000, 4000), (30000, 8)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("fixture.png");
            RgbaImage::from_pixel(width, height, image::Rgba([100, 120, 140, 255]))
                .save(&path)
                .unwrap();
            let start = Instant::now();
            let source = Arc::new(core_image::load_image(&path).unwrap().to_rgba8());
            let decode_ms = start.elapsed().as_secs_f64() * 1000.0;
            for tiered in [false, true] {
                let ctx = egui::Context::default();
                let mut display = DisplayTexture::default();
                let mut texture = None;
                let view = viewer_state::ViewportState::new();
                ctx.data_mut(|data| {
                    data.insert_temp(
                        egui::Id::new("benchmark").with("bounds"),
                        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0)),
                    )
                });
                let start = Instant::now();
                let mut frames = Vec::new();
                while texture.is_none() {
                    let frame_start = Instant::now();
                    let _ = ctx.run(
                        egui::RawInput {
                            max_texture_side: Some(8192),
                            ..Default::default()
                        },
                        |ctx| {
                            if tiered {
                                display.sync_view(
                                    ctx,
                                    Arc::clone(&source),
                                    &mut texture,
                                    "benchmark",
                                    &view,
                                    "benchmark",
                                );
                            } else {
                                display.sync(ctx, Arc::clone(&source), &mut texture, "benchmark");
                            }
                        },
                    );
                    frames.push(frame_start.elapsed().as_secs_f64() * 1000.0);
                    assert!(start.elapsed() < Duration::from_secs(60));
                    std::thread::sleep(Duration::from_millis(1));
                }
                frames.sort_by(f64::total_cmp);
                let size = texture.unwrap().size();
                println!(
                    "RESOURCE_BASELINE {}",
                    serde_json::json!({"dimensions":[width,height],"tiered":tiered,"decode_ms":decode_ms,"first_texture_ms":start.elapsed().as_secs_f64()*1000.0,"cpu_frame_p95_ms":frames[(frames.len()-1)*95/100],"source_rgba_bytes":source.as_raw().len(),"texture_rgba_estimate_bytes":size[0]*size[1]*4})
                );
            }
        }
    }

    #[test]
    fn texture_matches_latest_source_and_device_limit() {
        let ctx = egui::Context::default();
        let mut display = DisplayTexture::default();
        let mut texture = None;
        let old = Arc::new(RgbaImage::new(4000, 10));
        let current = Arc::new(RgbaImage::new(4, 2000));
        let start = Instant::now();
        for source in [&old, &current] {
            let _ = ctx.run(
                egui::RawInput {
                    max_texture_side: Some(1024),
                    ..Default::default()
                },
                |ctx| {
                    display.sync(ctx, Arc::clone(source), &mut texture, "test");
                },
            );
        }
        while texture.is_none() {
            let _ = ctx.run(Default::default(), |ctx| {
                display.sync(ctx, Arc::clone(&current), &mut texture, "test")
            });
            assert!(start.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(texture.unwrap().size(), [2, 1024]);
        assert_eq!(current.dimensions(), (4, 2000));
    }
}
