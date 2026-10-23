//! 有界保形平滑曲線；只映射 RGB，完全保留 Alpha。
use image::RgbaImage;
use rayon::prelude::*;

pub const MAX_CURVE_POINTS: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CurveInterpolation {
    Linear,
    #[default]
    Smooth,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CurvePoint {
    pub input: u8,
    pub output: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Curve {
    points: [CurvePoint; MAX_CURVE_POINTS],
    len: u8,
}
impl Default for Curve {
    fn default() -> Self {
        let mut points = [CurvePoint::default(); MAX_CURVE_POINTS];
        points[1] = CurvePoint {
            input: 255,
            output: 255,
        };
        Self { points, len: 2 }
    }
}
impl Curve {
    pub fn new(points: &[CurvePoint]) -> Result<Self, &'static str> {
        if !(2..=MAX_CURVE_POINTS).contains(&points.len()) {
            return Err("每條曲線需要 2–16 個控制點");
        }
        if points[0].input != 0 || points[points.len() - 1].input != 255 {
            return Err("曲線必須包含輸入為 0 與 255 的端點");
        }
        if points.windows(2).any(|p| p[0].input >= p[1].input) {
            return Err("曲線輸入位置必須嚴格遞增，不可重複");
        }
        let mut curve = Self {
            points: [CurvePoint::default(); MAX_CURVE_POINTS],
            len: points.len() as u8,
        };
        curve.points[..points.len()].copy_from_slice(points);
        Ok(curve)
    }
    pub fn points(&self) -> &[CurvePoint] {
        &self.points[..self.len as usize]
    }
    pub fn is_identity(&self) -> bool {
        self.points().iter().all(|p| p.input == p.output)
    }
    pub fn lut(&self) -> [u8; 256] {
        self.lut_with(CurveInterpolation::Smooth)
    }
    pub fn lut_with(&self, interpolation: CurveInterpolation) -> [u8; 256] {
        // 舊預設集與兩點直線沿用整數取整，避免浮點半值誤差改變既有效果。
        if interpolation == CurveInterpolation::Linear || self.points().len() == 2 {
            let mut lut = [0; 256];
            for pair in self.points().windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let width = i32::from(b.input) - i32::from(a.input);
                for (input, value) in lut
                    .iter_mut()
                    .enumerate()
                    .take(b.input as usize + 1)
                    .skip(a.input as usize)
                {
                    let offset = input as i32 - i32::from(a.input);
                    let numerator = i32::from(a.output) * width
                        + (i32::from(b.output) - i32::from(a.output)) * offset;
                    *value = ((numerator + width / 2) / width) as u8;
                }
            }
            return lut;
        }
        self.samples(interpolation)
            .map(|value| value.round().clamp(0.0, 255.0) as u8)
    }
    /// 圖形與像素查表共用相同取樣，保形三次插值避免控制點間過衝。
    pub fn samples(&self, interpolation: CurveInterpolation) -> [f64; 256] {
        let points = self.points();
        let count = points.len();
        let mut widths = [0.0; MAX_CURVE_POINTS];
        let mut slopes = [0.0; MAX_CURVE_POINTS];
        let mut tangents = [0.0; MAX_CURVE_POINTS];
        for (i, pair) in points.windows(2).enumerate() {
            widths[i] = f64::from(pair[1].input) - f64::from(pair[0].input);
            slopes[i] = (f64::from(pair[1].output) - f64::from(pair[0].output)) / widths[i];
        }
        tangents[0] = slopes[0];
        tangents[count - 1] = slopes[count - 2];
        if count > 2 {
            let endpoint = |h0: f64, h1: f64, d0: f64, d1: f64| {
                let tangent = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
                if tangent * d0 <= 0.0 {
                    0.0
                } else if d0 * d1 <= 0.0 && tangent.abs() > 3.0 * d0.abs() {
                    3.0 * d0
                } else {
                    tangent
                }
            };
            tangents[0] = endpoint(widths[0], widths[1], slopes[0], slopes[1]);
            tangents[count - 1] = endpoint(
                widths[count - 2],
                widths[count - 3],
                slopes[count - 2],
                slopes[count - 3],
            );
            for i in 1..count - 1 {
                let (left, right) = (slopes[i - 1], slopes[i]);
                if left * right > 0.0 {
                    let w1 = 2.0 * widths[i] + widths[i - 1];
                    let w2 = widths[i] + 2.0 * widths[i - 1];
                    tangents[i] = (w1 + w2) / (w1 / left + w2 / right);
                }
            }
        }
        let mut values = [0.0; 256];
        for (i, pair) in points.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            for (input, value) in values
                .iter_mut()
                .enumerate()
                .take(b.input as usize + 1)
                .skip(a.input as usize)
            {
                let t = (input as f64 - f64::from(a.input)) / widths[i];
                let (y0, y1) = (f64::from(a.output), f64::from(b.output));
                *value = match interpolation {
                    CurveInterpolation::Linear => y0 + (y1 - y0) * t,
                    CurveInterpolation::Smooth => {
                        let t2 = t * t;
                        let t3 = t2 * t;
                        (2.0 * t3 - 3.0 * t2 + 1.0) * y0
                            + (t3 - 2.0 * t2 + t) * widths[i] * tangents[i]
                            + (-2.0 * t3 + 3.0 * t2) * y1
                            + (t3 - t2) * widths[i] * tangents[i + 1]
                    }
                }
                .clamp(y0.min(y1), y0.max(y1));
            }
        }
        values
    }
}

/// 四條曲線以固定容量快照傳遞，避免無界配置與額外生命週期。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CurveSet {
    pub interpolation: CurveInterpolation,
    pub rgb: Curve,
    pub red: Curve,
    pub green: Curve,
    pub blue: Curve,
}
impl CurveSet {
    pub fn is_identity(&self) -> bool {
        self.rgb.is_identity()
            && self.red.is_identity()
            && self.green.is_identity()
            && self.blue.is_identity()
    }
    pub fn apply_in_place(&self, image: &mut RgbaImage) {
        if self.is_identity() {
            return;
        }
        let master = self.rgb.lut_with(self.interpolation);
        let channels = [
            self.red.lut_with(self.interpolation),
            self.green.lut_with(self.interpolation),
            self.blue.lut_with(self.interpolation),
        ];
        let luts: [[u8; 256]; 3] = std::array::from_fn(|channel| {
            std::array::from_fn(|value| channels[channel][master[value] as usize])
        });
        image.as_mut().par_chunks_exact_mut(4).for_each(|pixel| {
            for channel in 0..3 {
                pixel[channel] = luts[channel][pixel[channel] as usize];
            }
        });
    }
}
