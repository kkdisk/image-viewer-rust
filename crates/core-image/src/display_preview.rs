//! 顯示用等比下採樣，不改變編輯底圖或輸出像素。

use image::RgbaImage;

pub fn display_preview(image: &RgbaImage, max_side: u32) -> RgbaImage {
    let max_side = max_side.max(1);
    if image.width() <= max_side && image.height() <= max_side {
        return image.clone();
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
