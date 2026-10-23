use core_image::{
    AdjustmentParams, apply_adjustments_rgba,
    curves::{Curve, CurveInterpolation, CurvePoint, CurveSet},
};
fn curve(points: &[[u8; 2]]) -> Curve {
    Curve::new(
        &points
            .iter()
            .map(|p| CurvePoint {
                input: p[0],
                output: p[1],
            })
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
#[test]
fn identity_inverse_and_interpolation_have_exact_rounding() {
    assert_eq!(Curve::default().lut(), std::array::from_fn(|i| i as u8));
    assert_eq!(
        curve(&[[0, 255], [255, 0]]).lut(),
        std::array::from_fn(|i| 255 - i as u8)
    );
    let lut = curve(&[[0, 0], [128, 255], [255, 255]]).lut();
    assert_eq!(
        (lut[0], lut[1], lut[64], lut[128], lut[255]),
        (0, 3, 175, 255, 255)
    );
    assert_eq!(curve(&[[0, 0], [255, 128]]).lut()[1], 1);
}
#[test]
fn invalid_points_are_rejected_without_sorting_or_truncation() {
    for points in [
        vec![],
        vec![[0, 0]],
        vec![[1, 0], [255, 255]],
        vec![[0, 0], [254, 255]],
        vec![[0, 0], [128, 10], [128, 20], [255, 255]],
        vec![[0, 0], [200, 10], [100, 20], [255, 255]],
        (0..17)
            .map(|i| [if i == 16 { 255 } else { i * 15 }, 0])
            .collect(),
    ] {
        let p: Vec<_> = points
            .iter()
            .map(|p| CurvePoint {
                input: p[0],
                output: p[1],
            })
            .collect();
        assert!(Curve::new(&p).is_err());
    }
}
#[test]
fn curves_run_after_micro_adjustments_and_master_before_channels_preserving_alpha() {
    let source = image::RgbaImage::from_raw(
        3,
        1,
        vec![25, 50, 100, 0, 100, 150, 200, 128, 255, 0, 128, 255],
    )
    .unwrap();
    let mut params = AdjustmentParams {
        brightness: 1.2,
        ..Default::default()
    };
    params.curves = CurveSet {
        rgb: curve(&[[0, 255], [255, 0]]),
        red: curve(&[[0, 0], [255, 128]]),
        ..Default::default()
    };
    // 參考運算直接使用兩階段 LUT，並核對與反向組合不同。
    let mut expected = apply_adjustments_rgba(
        &source,
        &AdjustmentParams {
            brightness: 1.2,
            ..Default::default()
        },
    );
    for p in expected.pixels_mut() {
        p[0] = ((u32::from(255 - p[0]) * 128 + 127) / 255) as u8;
        p[1] = 255 - p[1];
        p[2] = 255 - p[2];
    }
    let result = apply_adjustments_rgba(&source, &params);
    assert_eq!(result, expected);
    assert_eq!(
        result.pixels().map(|p| p[3]).collect::<Vec<_>>(),
        vec![0, 128, 255]
    );
    assert_ne!(
        params.curves.red.lut()[params.curves.rgb.lut()[50] as usize],
        params.curves.rgb.lut()[params.curves.red.lut()[50] as usize]
    );
}
#[test]
fn collinear_identity_points_leave_all_bytes_unchanged() {
    let source = image::RgbaImage::from_fn(256, 1, |x, _| {
        image::Rgba([x as u8, 255 - x as u8, 77, x as u8])
    });
    let params = AdjustmentParams {
        curves: CurveSet {
            rgb: curve(&[[0, 0], [100, 100], [255, 255]]),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(params.is_identity());
    assert_eq!(apply_adjustments_rgba(&source, &params), source);
}

#[test]
fn smooth_curve_passes_through_points_without_a_corner_or_overshoot() {
    let curve = curve(&[[0, 0], [145, 98], [255, 255]]);
    let samples = curve.samples(CurveInterpolation::Smooth);
    assert_eq!((samples[0], samples[145], samples[255]), (0.0, 98.0, 255.0));
    assert!(
        (samples[72] - 98.0 * 72.0 / 145.0).abs() > 5.0,
        "中間值必須形成曲線而非折線"
    );
    let incoming = samples[145] - samples[144];
    let outgoing = samples[146] - samples[145];
    assert!(
        (incoming - outgoing).abs() < 0.05,
        "控制點兩側斜率應平滑銜接"
    );
    assert!(samples.windows(2).all(|p| p[1] >= p[0]));
    for points in [
        vec![[0, 0], [70, 220], [150, 40], [255, 255]],
        vec![[0, 255], [1, 0], [254, 0], [255, 255]],
    ] {
        let samples = self::curve(&points).samples(CurveInterpolation::Smooth);
        assert!(
            samples
                .iter()
                .all(|v| v.is_finite() && (0.0..=255.0).contains(v))
        );
        for pair in points.windows(2) {
            let lower = f64::from(pair[0][1].min(pair[1][1]));
            let upper = f64::from(pair[0][1].max(pair[1][1]));
            assert!(
                samples[pair[0][0] as usize..=pair[1][0] as usize]
                    .iter()
                    .all(|v| *v >= lower && *v <= upper)
            );
        }
    }
}

#[test]
fn legacy_linear_curve_retains_the_original_mapping() {
    let curve = curve(&[[0, 0], [128, 255], [255, 255]]);
    assert_eq!(curve.lut_with(CurveInterpolation::Linear)[64], 128);
    assert_eq!(curve.lut()[64], 175);
    for interpolation in [CurveInterpolation::Linear, CurveInterpolation::Smooth] {
        assert_eq!(
            Curve::default().lut_with(interpolation),
            std::array::from_fn(|i| i as u8)
        );
    }
}
