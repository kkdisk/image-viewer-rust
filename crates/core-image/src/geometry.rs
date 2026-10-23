//! 不改變來源像素的旋轉與翻轉，保留 Alpha。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    Clockwise,
    Counterclockwise,
    Horizontal,
    Vertical,
}

/// 完整已提交影像的整數裁切範圍，右下邊界不包含在內。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeFilter {
    Cubic,
    Pixel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditOperation {
    Crop(CropRect),
    Resize {
        width: u32,
        height: u32,
        filter: ResizeFilter,
    },
}

impl EditOperation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crop(_) => "裁切圖片",
            Self::Resize { .. } => "變更圖片尺寸",
        }
    }

    pub fn apply(self, source: &image::RgbaImage) -> Result<image::RgbaImage, crate::ImageError> {
        use crate::{ImageError, MAX_SAFE_PIXELS, check_pixel_limit};
        match self {
            Self::Crop(r) => {
                if r.width == 0
                    || r.height == 0
                    || u64::from(r.x) + u64::from(r.width) > u64::from(source.width())
                    || u64::from(r.y) + u64::from(r.height) > u64::from(source.height())
                {
                    return Err(ImageError::InvalidEdit("裁切範圍超出影像或為空"));
                }
                Ok(image::imageops::crop_imm(source, r.x, r.y, r.width, r.height).to_image())
            }
            Self::Resize {
                width,
                height,
                filter,
            } => {
                if width == 0 || height == 0 {
                    return Err(ImageError::InvalidEdit("尺寸必須大於零"));
                }
                check_pixel_limit(width, height, MAX_SAFE_PIXELS)?;
                if source.dimensions() == (width, height) {
                    return Ok(source.clone());
                }
                if filter == ResizeFilter::Pixel {
                    return Ok(image::imageops::resize(
                        source,
                        width,
                        height,
                        image::imageops::FilterType::Nearest,
                    ));
                }
                // 透明圖預乘後取樣，避免全透明像素的隱藏色形成邊線。
                let mut pixels = source.clone();
                for pixel in pixels.pixels_mut() {
                    let alpha = u16::from(pixel[3]);
                    for channel in &mut pixel.0[..3] {
                        *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                    }
                }
                let mut result = image::imageops::resize(
                    &pixels,
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
                Ok(result)
            }
        }
    }
}

/// 主值改變時按原比例四捨五入，使用寬整數避免乘法溢位。
pub fn proportional_dimension(value: u32, numerator: u32, denominator: u32) -> Option<u32> {
    if value == 0 || denominator == 0 {
        return None;
    }
    let result = (u64::from(value) * u64::from(numerator) + u64::from(denominator) / 2)
        / u64::from(denominator);
    u32::try_from(result.max(1)).ok()
}
impl Transform {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clockwise => "順時針旋轉 90 度",
            Self::Counterclockwise => "逆時針旋轉 90 度",
            Self::Horizontal => "水平翻轉",
            Self::Vertical => "垂直翻轉",
        }
    }
    pub fn apply(self, image: &image::RgbaImage) -> image::RgbaImage {
        match self {
            Self::Clockwise => image::imageops::rotate90(image),
            Self::Counterclockwise => image::imageops::rotate270(image),
            Self::Horizontal => image::imageops::flip_horizontal(image),
            Self::Vertical => image::imageops::flip_vertical(image),
        }
    }
}
