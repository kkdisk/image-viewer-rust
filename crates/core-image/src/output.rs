//! 明確輸出設定；不複製來源元資料，透明 JPEG 明確合成背景。
use crate::ImageError;
use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Png,
    Jpeg,
}

#[derive(Debug, Clone, Copy)]
pub struct OutputOptions {
    pub format: OutputFormat,
    pub quality: u8,
    pub background: [u8; 3],
}
impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Png,
            quality: 90,
            background: [255; 3],
        }
    }
}
impl OutputOptions {
    pub fn matches_path(self, path: &Path) -> bool {
        matches!(
            (self.format, ImageFormat::from_path(path)),
            (OutputFormat::Png, Ok(ImageFormat::Png)) | (OutputFormat::Jpeg, Ok(ImageFormat::Jpeg))
        )
    }
}

pub fn flatten(source: &RgbaImage, background: [u8; 3]) -> RgbImage {
    RgbImage::from_fn(source.width(), source.height(), |x, y| {
        let p = source.get_pixel(x, y);
        let a = u32::from(p[3]);
        image::Rgb(std::array::from_fn(|i| {
            ((u32::from(p[i]) * a + u32::from(background[i]) * (255 - a) + 127) / 255) as u8
        }))
    })
}

pub fn save_with_options(
    source: &RgbaImage,
    path: &Path,
    options: OutputOptions,
) -> Result<(), ImageError> {
    if !options.matches_path(path) {
        return Err(ImageError::InvalidEdit("輸出格式與副檔名不一致"));
    }
    if !(1..=100).contains(&options.quality) {
        return Err(ImageError::InvalidEdit("JPEG 品質必須介於 1 至 100"));
    }
    crate::io::write_atomic(path, |writer| {
        match options.format {
            OutputFormat::Png => {
                DynamicImage::ImageRgba8(source.clone()).write_to(writer, ImageFormat::Png)?
            }
            OutputFormat::Jpeg => {
                let pixels = flatten(source, options.background);
                image::codecs::jpeg::JpegEncoder::new_with_quality(writer, options.quality)
                    .encode_image(&DynamicImage::ImageRgb8(pixels))?;
            }
        }
        Ok(())
    })
}
