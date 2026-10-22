//! 不改變來源像素的旋轉與翻轉，保留 Alpha。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    Clockwise,
    Counterclockwise,
    Horizontal,
    Vertical,
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
