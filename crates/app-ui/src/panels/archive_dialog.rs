//! 壓縮檔密碼輸入對話框模組
//! 當開啟帶有密碼保護之 7z 或 zip 壓縮包時，彈出對話框提示使用者輸入密碼進行解密

use crate::theme::ThemeColors;
use egui::{Color32, Margin, RichText, Rounding, Stroke, TextEdit, Window};
use std::path::Path;

/// 密碼對話框操作動作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchivePasswordAction {
    /// 使用者輸入密碼並點擊確認
    Confirm(String),
    /// 使用者取消操作
    Cancel,
    /// 無操作
    None,
}

/// 壓縮檔密碼輸入彈出視窗
pub struct ArchivePasswordDialog<'a> {
    pub archive_path: &'a Path,
    pub password_input: &'a mut String,
    pub password_error: Option<&'a str>,
    pub show_password_char: &'a mut bool,
    pub is_open: &'a mut bool,
}

impl<'a> ArchivePasswordDialog<'a> {
    pub fn new(
        archive_path: &'a Path,
        password_input: &'a mut String,
        password_error: Option<&'a str>,
        show_password_char: &'a mut bool,
        is_open: &'a mut bool,
    ) -> Self {
        Self {
            archive_path,
            password_input,
            password_error,
            show_password_char,
            is_open,
        }
    }

    /// 繪製對話框並回傳使用者觸發之動作
    pub fn show(self, ctx: &egui::Context) -> ArchivePasswordAction {
        let mut action = ArchivePasswordAction::None;
        let mut open_flag = *self.is_open;

        let filename = self
            .archive_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("壓縮檔");

        Window::new("🔒 壓縮檔密碼保護")
            .open(&mut open_flag)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .fixed_size(egui::vec2(380.0, 200.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(Color32::from_rgb(26, 26, 30))
                    .stroke(Stroke::new(1.5_f32, ThemeColors::ACCENT))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("檔案: {}", filename))
                            .color(Color32::WHITE)
                            .strong(),
                    );
                    ui.label(
                        RichText::new("此壓縮檔已加密，請輸入密碼以解鎖瀏覽圖片：")
                            .color(Color32::from_rgb(200, 200, 210))
                            .size(13.0),
                    );

                    ui.add_space(10.0);

                    // 密碼輸入框與切換顯示
                    ui.horizontal(|ui| {
                        let is_mask = !*self.show_password_char;
                        let text_edit = TextEdit::singleline(self.password_input)
                            .password(is_mask)
                            .hint_text("請輸入解鎖密碼...")
                            .desired_width(ui.available_width() - 32.0);

                        let res = ui.add(text_edit);

                        // 首次開啟視窗時自動聚焦，避免後續持續搶焦點干擾點擊與 Tab
                        let focus_key = egui::Id::new("archive_password_dialog_init_focus");
                        let has_focused =
                            ui.data(|d| d.get_temp::<bool>(focus_key)).unwrap_or(false);
                        if !has_focused {
                            res.request_focus();
                            ui.data_mut(|d| d.insert_temp(focus_key, true));
                        }

                        // 按下 Enter 鍵直接觸發解鎖 (egui 單行輸入框按下 Enter 會同時 lost_focus)
                        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (res.has_focus() || res.lost_focus()) && enter_pressed {
                            action = ArchivePasswordAction::Confirm(self.password_input.clone());
                        }

                        // 快速切換顯示/隱藏眼睛按鈕
                        let eye_icon = if *self.show_password_char {
                            "👁"
                        } else {
                            "🔒"
                        };
                        if ui
                            .small_button(eye_icon)
                            .on_hover_text("切換顯示/隱藏密碼")
                            .clicked()
                        {
                            *self.show_password_char = !*self.show_password_char;
                        }
                    });

                    ui.add_space(4.0);

                    // 密碼長度與狀態輔助提示
                    ui.horizontal(|ui| {
                        ui.checkbox(self.show_password_char, "顯示密碼文字");

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let len = self.password_input.chars().count();
                            let color = if len > 0 {
                                ThemeColors::ACCENT_HOVER
                            } else {
                                Color32::from_rgb(140, 140, 150)
                            };
                            ui.label(
                                RichText::new(format!("長度: {} 字元", len))
                                    .size(11.0)
                                    .color(color),
                            );
                        });
                    });

                    // 錯誤訊息提示（使用純色塊與無豆腐塊文字）
                    if let Some(err) = self.password_error {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("● {}", err))
                                .color(Color32::from_rgb(255, 95, 95))
                                .strong(),
                        );
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // 底部按鈕區
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let confirm_btn = ui.add_sized(
                                [90.0, 28.0],
                                egui::Button::new(
                                    RichText::new("解鎖確認").color(Color32::WHITE).strong(),
                                )
                                .fill(ThemeColors::ACCENT)
                                .rounding(Rounding::same(4.0)),
                            );

                            let cancel_btn = ui.add_sized(
                                [70.0, 28.0],
                                egui::Button::new(
                                    RichText::new("取消").color(Color32::from_rgb(200, 200, 200)),
                                )
                                .fill(Color32::from_rgb(50, 50, 58))
                                .rounding(Rounding::same(4.0)),
                            );

                            if confirm_btn.clicked() {
                                action =
                                    ArchivePasswordAction::Confirm(self.password_input.clone());
                            } else if cancel_btn.clicked() {
                                action = ArchivePasswordAction::Cancel;
                            }
                        });
                    });
                });
            });

        if !open_flag {
            action = ArchivePasswordAction::Cancel;
        }

        if action != ArchivePasswordAction::None {
            ctx.data_mut(|d| {
                d.remove_temp::<bool>(egui::Id::new("archive_password_dialog_init_focus"))
            });
        }

        *self.is_open = open_flag;
        action
    }
}
