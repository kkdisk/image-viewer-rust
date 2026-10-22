use app_ui::{
    app::ImageViewerApp,
    panels::{DocumentStatus, StatusBar, StatusBarState, ZoomAction},
};
use core_image::AdjustmentParams;
use image::{Rgba, RgbaImage};

fn loaded_app() -> ImageViewerApp {
    let mut app = ImageViewerApp::default();
    app.session = Some(viewer_state::EditingSession::new(
        RgbaImage::from_pixel(4, 4, Rgba([100, 0, 0, 255])),
        None,
    ));
    app
}

#[test]
fn preview_status_preserves_the_committed_save_state() {
    let mut app = loaded_app();
    assert_eq!(app.document_status(), DocumentStatus::Saved);
    app.slider_params.brightness = 1.5;
    assert_eq!(
        app.document_status(),
        DocumentStatus::Preview {
            committed_dirty: false,
            computing: false
        }
    );
    app.session
        .as_mut()
        .unwrap()
        .update_preview(app.slider_params);
    assert_eq!(
        app.document_status(),
        DocumentStatus::Preview {
            committed_dirty: false,
            computing: false
        }
    );
    assert!(app.has_unsaved_changes());
    assert!(app.apply_current_adjustments());
    assert_eq!(app.document_status(), DocumentStatus::Unsaved);
    app.slider_params.brightness = 1.2;
    app.session
        .as_mut()
        .unwrap()
        .update_preview(app.slider_params);
    assert_eq!(
        app.document_status(),
        DocumentStatus::Preview {
            committed_dirty: true,
            computing: false
        }
    );
    app.session.as_mut().unwrap().cancel_adjustments();
    app.slider_params = AdjustmentParams::default();
    assert_eq!(app.document_status(), DocumentStatus::Unsaved);
}

#[test]
fn zoom_controls_respect_limits_and_do_not_change_document_history() {
    let mut app = loaded_app();
    app.viewport.zoom = 0.25;
    app.apply_zoom_action(ZoomAction::In);
    assert!((app.viewport.zoom - 0.3).abs() < 0.0001);
    assert!(!app.viewport.auto_fit);
    app.apply_zoom_action(ZoomAction::Set(100.0));
    assert_eq!(app.viewport.zoom, 50.0);
    app.apply_zoom_action(ZoomAction::Set(f32::NAN));
    assert_eq!(app.viewport.zoom, 50.0);
    app.apply_zoom_action(ZoomAction::Actual);
    assert_eq!(app.viewport.zoom, 1.0);
    app.apply_zoom_action(ZoomAction::Fit);
    assert!(app.viewport.auto_fit);
    app.viewport.zoom = 80.0;
    app.apply_zoom_action(ZoomAction::In);
    assert_eq!(app.viewport.zoom, 80.0);
    assert!(app.viewport.auto_fit);
    assert!(!app.session.as_ref().unwrap().can_undo());
    assert!(!app.session.as_ref().unwrap().is_dirty());
}

fn label_position(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
    match shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            Some(text.pos + text.galley.size() * 0.5)
        }
        egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| label_position(shape, label)),
        _ => None,
    }
}

fn status_frame(
    ctx: &egui::Context,
    state: &StatusBarState<'_>,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, Option<ZoomAction>) {
    let mut action = None;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            action = StatusBar::show(ctx, state);
        },
    );
    (output, action)
}

#[test]
fn status_bar_buttons_work_with_pointer_events_and_are_disabled_without_an_image() {
    for (document, expected) in [
        (DocumentStatus::Saved, Some(ZoomAction::In)),
        (DocumentStatus::Empty, None),
    ] {
        let ctx = egui::Context::default();
        let state = StatusBarState {
            animated: false,
            filename: Some("非常長的圖片名稱.png"),
            dimensions: Some((4000, 3000)),
            zoom: 0.25,
            document,
            playlist_info: Some((0, 4)),
            notification: Some("正在背景載入圖片…"),
        };
        status_frame(&ctx, &state, vec![]);
        let (output, _) = status_frame(&ctx, &state, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| label_position(&shape.shape, "＋"))
            .unwrap();
        status_frame(
            &ctx,
            &state,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        let (_, action) = status_frame(
            &ctx,
            &state,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(action, expected);
    }
}

#[test]
fn histogram_channels_fit_narrow_panels_and_accept_pointer_selection() {
    use app_ui::panels::{HistogramMode, HistogramWidget};
    let ctx = egui::Context::default();
    app_ui::theme::apply_theme(&ctx);
    let mut mode = HistogramMode::Rgb;
    let mut frame = |events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(240.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    HistogramWidget::show(ui, None, &mut mode);
                });
            },
        )
    };
    frame(vec![]);
    let output = frame(vec![]);
    for label in ["RGB", "紅", "綠", "藍", "亮度"] {
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| label_position(&shape.shape, label))
            .unwrap();
        assert!(pos.x > 0.0 && pos.x < 240.0);
    }
    let pos = output
        .shapes
        .iter()
        .find_map(|shape| label_position(&shape.shape, "亮度"))
        .unwrap();
    frame(vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]);
    frame(vec![egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]);
    assert_eq!(mode, HistogramMode::Luma);
}

#[test]
fn adjustment_footer_stays_visible_at_minimum_window_size_and_blocks_early_apply() {
    use app_ui::panels::{AdjustmentAction, AdjustmentPanel, AdjustmentPanelState, HistogramMode};
    for computing in [false, true] {
        let ctx = egui::Context::default();
        app_ui::theme::apply_theme(&ctx);
        let (mut white_balance, mut fine_tune, mut open) = (true, true, true);
        let mut params = AdjustmentParams {
            brightness: 1.5,
            ..Default::default()
        };
        let mut mode = HistogramMode::Rgb;
        let mut action = None;
        let mut frame = |events| {
            action = None;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::TopBottomPanel::top("menu_test")
                        .exact_height(30.0)
                        .show(ctx, |_| {});
                    egui::TopBottomPanel::bottom("filmstrip_and_status_test")
                        .exact_height(185.0)
                        .show(ctx, |_| {});
                    AdjustmentPanel::show(
                        ctx,
                        &mut AdjustmentPanelState {
                            white_balance_open: &mut white_balance,
                            fine_tune_open: &mut fine_tune,
                            is_open: &mut open,
                            has_image: true,
                            params: &mut params,
                            is_computing: computing,
                            histogram_data: None,
                            histogram_mode: &mut mode,
                        },
                        &mut action,
                    );
                },
            )
        };
        // 先完成側欄的初始佈局，再比較捲動前後的位置。
        for _ in 0..4 {
            frame(vec![]);
        }
        let output = frame(vec![]);
        for label in ["重設", "取消", "套用"] {
            let pos = output
                .shapes
                .iter()
                .find_map(|shape| label_position(&shape.shape, label))
                .unwrap();
            assert!(
                pos.y < 415.0 && pos.y > 30.0,
                "按鈕超出可見側欄：{label} {pos:?}"
            );
        }
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| label_position(&shape.shape, "套用"))
            .unwrap();
        frame(vec![
            egui::Event::PointerMoved(egui::pos2(650.0, 180.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -1200.0),
                modifiers: Default::default(),
            },
        ]);
        let scrolled = frame(vec![]);
        let after_scroll = scrolled
            .shapes
            .iter()
            .find_map(|shape| label_position(&shape.shape, "套用"))
            .unwrap();
        assert!(
            (after_scroll - pos).length() < 0.1,
            "捲動控制時操作區不可移動：捲動前 {pos:?}，捲動後 {after_scroll:?}"
        );
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(
            action,
            if computing {
                None
            } else {
                Some(AdjustmentAction::Apply)
            }
        );
    }
}

#[test]
fn status_rendering_preserves_auto_fit_zoom_and_keeps_dimensions_visible_with_long_names() {
    let ctx = egui::Context::default();
    app_ui::theme::apply_theme(&ctx);
    let filename = format!("{}.png", "很長的圖片名稱".repeat(30));
    let message = "載入訊息".repeat(80);
    let state = StatusBarState {
        animated: false,
        filename: Some(&filename),
        dimensions: Some((6400, 4800)),
        zoom: 80.0,
        document: DocumentStatus::Preview {
            committed_dirty: true,
            computing: true,
        },
        playlist_info: Some((0, 500)),
        notification: Some(&message),
    };
    status_frame(&ctx, &state, vec![]);
    let (output, action) = status_frame(&ctx, &state, vec![]);
    assert_eq!(
        action, None,
        "繪製不能將自動符合視窗的倍率夾限或改成手動縮放"
    );
    for label in ["6400 × 4800 px", "預覽運算中", "底圖未儲存"] {
        let position = output
            .shapes
            .iter()
            .find_map(|shape| label_position(&shape.shape, label))
            .unwrap();
        assert!(position.x > 0.0 && position.x < 800.0);
    }
}
