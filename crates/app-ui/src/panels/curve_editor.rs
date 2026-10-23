//! 曲線圖形與數值編輯；只更新參數，像素仍由共用背景工作產生。
use core_image::curves::{Curve, CurveInterpolation, CurvePoint, CurveSet, MAX_CURVE_POINTS};
use egui::{Color32, Pos2, Rect, Stroke, Ui, pos2, vec2};

fn to_screen(rect: Rect, point: CurvePoint) -> Pos2 {
    pos2(
        rect.left() + rect.width() * f32::from(point.input) / 255.0,
        rect.bottom() - rect.height() * f32::from(point.output) / 255.0,
    )
}
fn from_screen(rect: Rect, pos: Pos2) -> CurvePoint {
    CurvePoint {
        input: ((pos.x - rect.left()) / rect.width() * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
        output: ((rect.bottom() - pos.y) / rect.height() * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
    }
}
fn move_point(points: &mut [CurvePoint], index: usize, point: CurvePoint) {
    let last = points.len() - 1;
    let input = if index == 0 {
        0
    } else if index == last {
        255
    } else {
        point
            .input
            .clamp(points[index - 1].input + 1, points[index + 1].input - 1)
    };
    points[index] = CurvePoint {
        input,
        output: point.output,
    };
}

pub fn show(ui: &mut Ui, curves: &mut CurveSet) -> bool {
    let before_interpolation = curves.interpolation;
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut curves.interpolation,
            CurveInterpolation::Smooth,
            "平滑",
        );
        ui.selectable_value(
            &mut curves.interpolation,
            CurveInterpolation::Linear,
            "折線",
        );
    });
    let interpolation = curves.interpolation;
    let channel_id = ui.make_persistent_id("curve_channel");
    let selection_id = ui.make_persistent_id("curve_selected_point");
    let mut channel = ui
        .ctx()
        .data_mut(|d| d.get_temp::<usize>(channel_id).unwrap_or(0));
    let mut selected = ui
        .ctx()
        .data_mut(|d| d.get_temp::<Option<usize>>(selection_id).flatten());
    let before_channel = channel;
    ui.horizontal(|ui| {
        for (i, label) in ["RGB", "紅", "綠", "藍"].iter().enumerate() {
            ui.selectable_value(&mut channel, i, *label);
        }
    });
    if before_channel != channel {
        selected = None;
    }
    let curve = match channel {
        1 => &mut curves.red,
        2 => &mut curves.green,
        3 => &mut curves.blue,
        _ => &mut curves.rgb,
    };
    let before = *curve;
    let mut points = curve.points().to_vec();
    selected = selected.filter(|i| *i < points.len());
    let color = match channel {
        1 => Color32::from_rgb(248, 113, 113),
        2 => Color32::from_rgb(74, 222, 128),
        3 => Color32::from_rgb(96, 165, 250),
        _ => Color32::from_rgb(230, 230, 235),
    };
    let (bounds, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), 164.0), egui::Sense::hover());
    let response = ui.interact(
        bounds,
        ui.make_persistent_id("curve_graph"),
        egui::Sense::click_and_drag(),
    );
    let rect = bounds.shrink(8.0);
    let painter = ui.painter().with_clip_rect(bounds);
    painter.rect_filled(bounds, 4.0, crate::theme::ThemeColors::BG_CANVAS);
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        painter.line_segment(
            [
                pos2(rect.left() + rect.width() * t, rect.top()),
                pos2(rect.left() + rect.width() * t, rect.bottom()),
            ],
            Stroke::new(1.0_f32, crate::theme::ThemeColors::BORDER),
        );
        painter.line_segment(
            [
                pos2(rect.left(), rect.top() + rect.height() * t),
                pos2(rect.right(), rect.top() + rect.height() * t),
            ],
            Stroke::new(1.0_f32, crate::theme::ThemeColors::BORDER),
        );
    }
    let nearest = |pos: Pos2, points: &[CurvePoint]| {
        points
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let distance = to_screen(rect, *p).distance(pos);
                (distance <= 12.0).then_some((i, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    if response.drag_started()
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        selected = nearest(origin, &points);
    }
    if response.dragged() {
        if let (Some(index), Some(pos)) = (selected, response.interact_pointer_pos()) {
            move_point(&mut points, index, from_screen(rect, pos));
        }
    } else if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        selected = nearest(pos, &points);
        if selected.is_none() && points.len() < MAX_CURVE_POINTS {
            let point = from_screen(rect, pos);
            let index = points.partition_point(|p| p.input < point.input);
            if index > 0 && index < points.len() && points[index].input != point.input {
                points.insert(index, point);
                selected = Some(index);
            }
        }
    }
    if let Ok(display_curve) = Curve::new(&points) {
        let samples = display_curve.samples(interpolation);
        let positions: Vec<_> = samples
            .iter()
            .enumerate()
            .map(|(input, output)| {
                pos2(
                    rect.left() + rect.width() * input as f32 / 255.0,
                    rect.bottom() - rect.height() * *output as f32 / 255.0,
                )
            })
            .collect();
        painter.add(egui::Shape::line(positions, Stroke::new(2.0_f32, color)));
    }
    for (index, point) in points.iter().enumerate() {
        painter.circle_filled(
            to_screen(rect, *point),
            if selected == Some(index) { 5.5 } else { 4.0 },
            color,
        );
    }
    response.on_hover_text("點擊空白處新增控制點，拖曳圓點調整；選取內點後可刪除");
    if let Some(index) = selected {
        let mut point = points[index];
        ui.horizontal(|ui| {
            ui.label("輸入");
            ui.add_enabled(
                index > 0 && index + 1 < points.len(),
                egui::DragValue::new(&mut point.input).range(0..=255),
            );
            ui.label("輸出");
            ui.add(egui::DragValue::new(&mut point.output).range(0..=255));
        });
        move_point(&mut points, index, point);
    }
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                selected.is_some_and(|i| i > 0 && i + 1 < points.len()),
                egui::Button::new("刪除控制點"),
            )
            .clicked()
        {
            points.remove(selected.unwrap());
            selected = None;
        }
        if ui.button("重設此通道").clicked() {
            points = Curve::default().points().to_vec();
            selected = None;
        }
    });
    if let Ok(updated) = Curve::new(&points) {
        *curve = updated;
    }
    ui.ctx().data_mut(|d| {
        d.insert_temp(channel_id, channel);
        d.insert_temp(selection_id, selected);
    });
    *curve != before || interpolation != before_interpolation
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn point_drags_keep_endpoints_and_neighbours_ordered_at_multiple_dpi() {
        let mut points = vec![
            CurvePoint {
                input: 0,
                output: 0,
            },
            CurvePoint {
                input: 128,
                output: 128,
            },
            CurvePoint {
                input: 255,
                output: 255,
            },
        ];
        for scale in [1.0, 2.5, 4.0] {
            let rect = Rect::from_min_size(pos2(5.0, 10.0), vec2(240.0 * scale, 160.0 * scale));
            for point in points.clone() {
                assert_eq!(from_screen(rect, to_screen(rect, point)), point);
            }
            move_point(
                &mut points,
                1,
                CurvePoint {
                    input: 255,
                    output: 200,
                },
            );
            assert_eq!(points[1].input, 254);
            move_point(
                &mut points,
                0,
                CurvePoint {
                    input: 55,
                    output: 20,
                },
            );
            assert_eq!(points[0].input, 0);
            assert!(Curve::new(&points).is_ok());
        }
    }
    #[test]
    fn pointer_add_and_drag_preserve_other_curves() {
        for pixels_per_point in [1.0, 2.0] {
            let ctx = egui::Context::default();
            crate::theme::apply_theme(&ctx);
            ctx.set_pixels_per_point(pixels_per_point);
            let mut curves = CurveSet::default();
            let mut frame = |events| {
                let mut graph = Rect::NOTHING;
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(340.0, 600.0))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            let id = ui.make_persistent_id("curve_graph");
                            show(ui, &mut curves);
                            if let Some(response) = ctx.read_response(id) {
                                graph = response.rect;
                            }
                        });
                    },
                );
                graph
            };
            for _ in 0..3 {
                frame(vec![]);
            }
            let rect = frame(vec![]).shrink(8.0);
            let point = to_screen(
                rect,
                CurvePoint {
                    input: 128,
                    output: 200,
                },
            );
            for pressed in [true, false] {
                frame(vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ]);
            }
            let moved = to_screen(
                rect,
                CurvePoint {
                    input: 180,
                    output: 80,
                },
            );
            frame(vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            }]);
            frame(vec![egui::Event::PointerMoved(moved)]);
            frame(vec![]);
            frame(vec![egui::Event::PointerButton {
                pos: moved,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }]);
            assert_eq!(curves.rgb.points().len(), 3);
            let p = curves.rgb.points()[1];
            assert!((i16::from(p.input) - 180).abs() <= 1 && (i16::from(p.output) - 80).abs() <= 1);
            assert!(
                curves.red.is_identity() && curves.green.is_identity() && curves.blue.is_identity()
            );
        }
    }
}
