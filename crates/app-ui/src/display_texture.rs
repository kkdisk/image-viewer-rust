//! 顯示紋理背景轉換；只保留最新等待快照，交付時再次核對影像及裝置上限。

use image::RgbaImage;
use std::sync::{Arc, Mutex, mpsc};

#[derive(Clone, Copy, PartialEq, Eq)]
struct DisplayKey {
    crop: [u32; 4],
    limit: u32,
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
                    let pixels = core_image::display_preview::display_preview(&*crop, key.limit);
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
            region: egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        }
    }
}

impl DisplayTexture {
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
        let key = display_key(source.dimensions(), view, rect, ctx.pixels_per_point(), gpu);
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
                *texture = Some(ctx.load_texture(name, result.color, egui::TextureOptions::LINEAR));
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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
