//! 自訂現代暗色主題與視覺風格配置

use egui::{Color32, Margin, Rounding, Stroke, Style, Visuals, epaint::Shadow};

/// 現代深色系色彩調色盤
#[allow(dead_code)]
pub struct ThemeColors;

#[allow(dead_code)]
impl ThemeColors {
    pub const BG_APP: Color32 = Color32::from_rgb(18, 18, 20); // 最深底層背景
    pub const BG_PANEL: Color32 = Color32::from_rgb(26, 26, 30); // 面板/側欄背景
    pub const BG_CANVAS: Color32 = Color32::from_rgb(14, 14, 16); // 中央畫布底色
    pub const BG_WIDGET: Color32 = Color32::from_rgb(38, 38, 44); // 元件背景
    pub const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(52, 52, 60); // 元件懸停背景
    pub const BG_WIDGET_ACTIVE: Color32 = Color32::from_rgb(66, 66, 76); // 元件按下背景

    pub const ACCENT: Color32 = Color32::from_rgb(59, 130, 246); // 亮藍重音色 (Blue-500)
    pub const ACCENT_HOVER: Color32 = Color32::from_rgb(96, 165, 250); // 淺藍重音色 (Blue-400)

    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(244, 244, 245); // 主文字 (Zinc-100)
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(180, 180, 190); // 次要文字 (提升對比度)
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(130, 130, 140); // 暗淡文字 (提升對比度)

    pub const BORDER: Color32 = Color32::from_rgb(45, 45, 52); // 分隔邊框色
    pub const BORDER_FOCUS: Color32 = Color32::from_rgb(59, 130, 246); // 焦點邊框
    pub const SUCCESS: Color32 = Color32::from_rgb(34, 197, 94); // 成功綠
    pub const WARNING: Color32 = Color32::from_rgb(234, 179, 8); // 警示黃
}

/// 配置繁體中文字型支援（載入微軟正黑體等系統字型以杜絕方框豆腐塊）
pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Windows 繁體中文與 CJK 候選字型路徑
    let candidate_fonts = [
        ("msjh", r"C:\Windows\Fonts\msjh.ttc"), // 微軟正黑體 (首選)
        ("msjh_light", r"C:\Windows\Fonts\msjhl.ttc"), // 微軟正黑體 Light
        ("mingliu", r"C:\Windows\Fonts\mingliu.ttc"), // 細明體
    ];

    for (font_name, font_path) in candidate_fonts {
        if let Ok(font_bytes) = std::fs::read(font_path) {
            fonts
                .font_data
                .insert(font_name.to_owned(), egui::FontData::from_owned(font_bytes));

            // 將中文字型置於 Proportional 無襯線字型優先清單最前列
            if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                prop.insert(0, font_name.to_owned());
            }

            // 同步加入 Monospace 等寬字型作為備用
            if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                mono.push(font_name.to_owned());
            }

            ctx.set_fonts(fonts);
            break;
        }
    }
}

/// 套用專業暗色主題視覺設定至 egui::Context
pub fn apply_theme(ctx: &egui::Context) {
    // 優先設定中文字型
    setup_custom_fonts(ctx);

    let mut visuals = Visuals::dark();

    // 背景底色
    visuals.panel_fill = ThemeColors::BG_PANEL;
    visuals.window_fill = ThemeColors::BG_PANEL;
    visuals.extreme_bg_color = ThemeColors::BG_CANVAS;

    // 圓角設定 (細緻微圓角)
    let rounding = Rounding::same(6.0);
    visuals.window_rounding = Rounding::same(8.0);
    visuals.menu_rounding = Rounding::same(6.0);

    // 元件外觀 (Widgets)
    visuals.widgets.noninteractive.bg_fill = ThemeColors::BG_WIDGET;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, ThemeColors::BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, ThemeColors::TEXT_SECONDARY);
    visuals.widgets.noninteractive.rounding = rounding;

    visuals.widgets.inactive.bg_fill = ThemeColors::BG_WIDGET;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, ThemeColors::BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, ThemeColors::TEXT_PRIMARY);
    visuals.widgets.inactive.rounding = rounding;

    visuals.widgets.hovered.bg_fill = ThemeColors::BG_WIDGET_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ThemeColors::ACCENT_HOVER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.hovered.rounding = rounding;

    visuals.widgets.active.bg_fill = ThemeColors::BG_WIDGET_ACTIVE;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, ThemeColors::ACCENT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.rounding = rounding;

    visuals.widgets.open.bg_fill = ThemeColors::BG_WIDGET_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, ThemeColors::BORDER);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.open.rounding = rounding;

    // 陰影與選取高亮
    visuals.selection.bg_fill = ThemeColors::ACCENT;
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;

    // 間距配置
    let mut style = Style::clone(&*ctx.style());
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.window_margin = Margin::same(8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.visuals = visuals;

    ctx.set_style(style);
}
