//! 顯示紋理背景轉換；只保留最新等待快照，交付時再次核對影像及裝置上限。

use image::RgbaImage;
use std::sync::{Arc, Mutex, mpsc};

type Request = (Arc<RgbaImage>, u32);
struct ResultImage {
    source: Arc<RgbaImage>,
    limit: u32,
    color: egui::ColorImage,
}

pub(crate) struct DisplayTexture {
    pending: Arc<Mutex<Option<Request>>>,
    wake: mpsc::SyncSender<()>,
    results: mpsc::Receiver<ResultImage>,
    requested: Option<Request>,
    ready: bool,
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
                if let Some((source, limit)) = request {
                    let pixels = core_image::display_preview::display_preview(&source, limit);
                    let color = egui::ColorImage::from_rgba_unmultiplied(
                        [pixels.width() as usize, pixels.height() as usize],
                        pixels.as_raw(),
                    );
                    if sender
                        .send(ResultImage {
                            source,
                            limit,
                            color,
                        })
                        .is_err()
                    {
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
        }
    }
}

impl DisplayTexture {
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
        let changed = !self
            .requested
            .as_ref()
            .is_some_and(|(image, old_limit)| Arc::ptr_eq(image, &source) && *old_limit == limit);
        if changed {
            self.requested = Some((Arc::clone(&source), limit));
            self.ready = false;
            *texture = None;
            if let Ok(mut pending) = self.pending.lock() {
                *pending = Some((Arc::clone(&source), limit));
            }
            let _ = self.wake.try_send(());
        }
        while let Ok(result) = self.results.try_recv() {
            if Arc::ptr_eq(&result.source, &source) && result.limit == limit {
                *texture = Some(ctx.load_texture(name, result.color, egui::TextureOptions::LINEAR));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

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
