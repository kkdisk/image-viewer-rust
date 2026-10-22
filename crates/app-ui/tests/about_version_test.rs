use app_ui::panels::AboutDialog;

#[test]
fn about_dialog_displays_the_compiled_package_version() {
    fn contains_version(shape: &egui::Shape, expected: &str) -> bool {
        match shape {
            egui::Shape::Text(text) => text.galley.text() == expected,
            egui::Shape::Vec(shapes) => {
                shapes.iter().any(|shape| contains_version(shape, expected))
            }
            _ => false,
        }
    }

    let ctx = egui::Context::default();
    let expected = format!("版本 v{} (Rust 2024 Edition)", env!("CARGO_PKG_VERSION"));
    let mut open = true;
    // 首幀配置視窗，下一幀核對實際繪製的版本文字。
    let _ = ctx.run(Default::default(), |ctx| AboutDialog::show(ctx, &mut open));
    let output = ctx.run(Default::default(), |ctx| AboutDialog::show(ctx, &mut open));
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| contains_version(&shape.shape, &expected))
    );
}
