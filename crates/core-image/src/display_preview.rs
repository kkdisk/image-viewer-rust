//! 顯示用等比下採樣，不改變編輯底圖或輸出像素。

use image::{GenericImageView, Rgba, RgbaImage};

/// 顯示紋理最多 4096² RGBA8 像素（64 MiB），不限制編輯底圖尺寸。
pub fn bounded_display_size(size: [u32; 2], max_side: u32) -> [u32; 2] {
    let limit = max_side.clamp(1, 4096);
    let [width, height] = size.map(|value| value.max(1));
    let longest = u64::from(width.max(height));
    if longest <= u64::from(limit) {
        return [width, height];
    }
    [
        (u64::from(width) * u64::from(limit) / longest).max(1) as u32,
        (u64::from(height) * u64::from(limit) / longest).max(1) as u32,
    ]
}

/// 僅用於背景顯示；以雙三次插值重採樣，透明圖先預乘 Alpha，避免隱藏色滲出。
pub fn high_quality_preview<I: GenericImageView<Pixel = Rgba<u8>>>(
    image: &I,
    size: [u32; 2],
    max_side: u32,
) -> RgbaImage {
    let [width, height] = bounded_display_size(size, max_side);
    if image.dimensions() == (width, height) {
        return RgbaImage::from_fn(width, height, |x, y| image.get_pixel(x, y));
    }
    if image.pixels().all(|(_, _, pixel)| pixel[3] == 255) {
        return image::imageops::resize(
            image,
            width,
            height,
            image::imageops::FilterType::CatmullRom,
        );
    }
    let premultiplied = RgbaImage::from_fn(image.width(), image.height(), |x, y| {
        let mut pixel = image.get_pixel(x, y);
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
        pixel
    });
    let mut result = image::imageops::resize(
        &premultiplied,
        width,
        height,
        image::imageops::FilterType::CatmullRom,
    );
    for pixel in result.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = (u32::from(*channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    result
}

pub fn display_preview<I: GenericImageView<Pixel = Rgba<u8>>>(
    image: &I,
    max_side: u32,
) -> RgbaImage {
    let max_side = max_side.max(1);
    if image.width() <= max_side && image.height() <= max_side {
        return RgbaImage::from_fn(image.width(), image.height(), |x, y| image.get_pixel(x, y));
    }
    let longest = u64::from(image.width().max(image.height()));
    let width = (u64::from(image.width()) * u64::from(max_side) / longest).max(1) as u32;
    let height = (u64::from(image.height()) * u64::from(max_side) / longest).max(1) as u32;
    image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_quality_250_and_400_percent_match_cubic_and_keep_source() {
        let source = RgbaImage::from_fn(8, 4, |x, y| {
            Rgba([(x * x * 4) as u8, (y * 60) as u8, 0, 255])
        });
        let original = source.clone();
        for size in [[8, 4], [20, 10], [32, 16]] {
            let result = high_quality_preview(&source, size, 4096);
            assert_eq!(result.dimensions(), (size[0], size[1]));
            assert_eq!(
                result,
                image::imageops::resize(
                    &source,
                    size[0],
                    size[1],
                    image::imageops::FilterType::CatmullRom
                )
            );
        }
        assert_eq!(source, original);
        assert_ne!(
            high_quality_preview(&source, [20, 10], 4096),
            image::imageops::resize(&source, 20, 10, image::imageops::FilterType::Triangle)
        );
    }
    #[test]
    fn transparent_hidden_color_does_not_bleed_and_bounds_are_enforced() {
        let source = RgbaImage::from_fn(2, 1, |x, _| {
            if x == 0 {
                Rgba([0, 0, 255, 255])
            } else {
                Rgba([255, 0, 0, 0])
            }
        });
        let output = high_quality_preview(&source, [8, 4], 4096);
        for pixel in output.pixels() {
            assert_eq!(pixel[0], 0);
            if pixel[3] > 0 {
                assert_eq!(pixel[2], 255);
            }
        }
        assert_eq!(
            high_quality_preview(&source, [20000, 10000], 64).dimensions(),
            (64, 32)
        );
        assert_eq!(
            bounded_display_size([u32::MAX, u32::MAX], u32::MAX),
            [4096, 4096]
        );
        assert_eq!(bounded_display_size([0, 0], 0), [1, 1]);
    }

    #[test]
    fn cropped_view_preserves_source_coordinates_without_copying_source() {
        let source = RgbaImage::from_fn(100, 100, |x, y| Rgba([x as u8, y as u8, 0, 255]));
        let view = image::imageops::crop_imm(&source, 10, 20, 30, 40);
        let preview = display_preview(&*view, 128);
        assert_eq!(preview.dimensions(), (30, 40));
        assert_eq!(preview.get_pixel(0, 0), source.get_pixel(10, 20));
        assert_eq!(source.dimensions(), (100, 100));
    }

    #[test]
    fn long_edges_fit_without_modifying_source() {
        for (width, height, expected) in [
            (100, 2, (16, 1)),
            (2, 100, (1, 16)),
            (16, 16, (16, 16)),
            (1, 1, (1, 1)),
        ] {
            let source = RgbaImage::new(width, height);
            assert_eq!(display_preview(&source, 16).dimensions(), expected);
            assert_eq!(source.dimensions(), (width, height));
        }
        assert_eq!(
            display_preview(&RgbaImage::new(9, 2), 0).dimensions(),
            (1, 1)
        );
    }
}
