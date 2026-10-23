//! 拍攝日期工具列與虛擬化縮圖總覽。
use crate::{
    app::ImageViewerApp,
    photo_index::PhotoIndex,
    playlist::{DateFilter, SortMode, SortOptions},
    theme::ThemeColors,
};
use egui::{Align2, FontId, Rect, pos2, vec2};

#[derive(Default)]
pub(crate) struct PhotoBrowser {
    pub grid_open: bool,
    index: PhotoIndex,
    revision: Option<u64>,
    request: Option<u64>,
    completed: bool,
    scope: Option<std::path::PathBuf>,
    day: String,
    days: Vec<String>,
    error: bool,
}

fn valid_day(day: &str) -> bool {
    let b = day.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
    {
        return false;
    }
    let year = day[..4].parse::<u32>().unwrap_or(0);
    let month = day[5..7].parse::<usize>().unwrap_or(0);
    let date = day[8..].parse::<u32>().unwrap_or(0);
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    year > 0 && (1..=12).contains(&month) && date > 0 && date <= days[month - 1]
}

impl ImageViewerApp {
    pub(crate) fn poll_photo_browser(&mut self, ctx: &egui::Context) {
        let revision = self.playlist.catalog_revision();
        let browser = &mut self.photo_browser;
        if browser.revision.is_some_and(|old| old != revision) {
            browser.index.cancel();
            browser.request = None;
            browser.revision = None;
            browser.completed = false;
        }
        let scope = self.playlist.cache_scope();
        if browser.scope != scope {
            browser.scope = scope;
            browser.day.clear();
            browser.days.clear();
            browser.error = false;
        }
        let needed = browser.grid_open
            || self.preferences.current.sort.mode == SortMode::Captured
            || self.playlist.date_filter() != &DateFilter::All;
        if needed && browser.revision != Some(revision) {
            browser.revision = Some(revision);
            browser.request = Some(browser.index.request(
                revision,
                browser.scope.clone(),
                self.playlist.all_files().to_vec(),
            ));
        }
        if let Some(result) = browser.index.poll()
            && result.revision == revision
            && browser.request == Some(result.id)
        {
            let filter = self.playlist.date_filter().clone();
            self.playlist.set_captured_dates(result.dates);
            browser.days = result.days;
            self.playlist
                .set_date_filter(filter, self.session.as_ref().and_then(|s| s.source_path()));
            browser.request = None;
            browser.completed = true;
        }
        if browser.request.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn apply_photo_filter(&mut self, filter: DateFilter) {
        self.stop_slideshow();
        self.playlist
            .set_date_filter(filter, self.session.as_ref().and_then(|s| s.source_path()));
    }

    fn request_photo_dates(&mut self) {
        self.photo_browser.revision = Some(self.playlist.catalog_revision());
        self.photo_browser.completed = false;
        self.photo_browser.request = Some(self.photo_browser.index.request(
            self.playlist.catalog_revision(),
            self.playlist.cache_scope(),
            self.playlist.all_files().to_vec(),
        ));
    }

    pub(crate) fn show_photo_toolbar(&mut self, ctx: &egui::Context) {
        // 日期控制以彈出面板呈現；開關及背景進度都不增減畫布高度。
        egui::TopBottomPanel::top("photo_browser_toolbar")
            .exact_height(40.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(!self.photo_browser.grid_open, "單張")
                        .clicked()
                    {
                        self.photo_browser.grid_open = false;
                    }
                    if ui
                        .add_enabled(
                            self.geometry.is_none(),
                            egui::SelectableLabel::new(self.photo_browser.grid_open, "縮圖總覽"),
                        )
                        .clicked()
                    {
                        self.stop_slideshow();
                        self.photo_browser.grid_open = true;
                    }
                    ui.separator();
                    if ui
                        .selectable_label(
                            self.preferences.current.sort.mode == SortMode::Captured,
                            "依拍攝時間排序",
                        )
                        .clicked()
                    {
                        self.set_sort_options(SortOptions {
                            mode: SortMode::Captured,
                            descending: self.preferences.current.sort.descending,
                        });
                    }
                    let mut descending = self.preferences.current.sort.descending;
                    if ui.checkbox(&mut descending, "降冪").changed() {
                        self.set_sort_options(SortOptions {
                            mode: self.preferences.current.sort.mode,
                            descending,
                        });
                    }
                    let summary = match self.playlist.date_filter() {
                        DateFilter::All => "日期篩選".to_owned(),
                        DateFilter::Undated => "無可用日期".to_owned(),
                        DateFilter::Day(day) => format!("日期：{day}"),
                    };
                    let active = self.playlist.date_filter() != &DateFilter::All;
                    let popup = ui.menu_button(
                        egui::RichText::new(summary).color(if active {
                            ThemeColors::ACCENT_HOVER
                        } else {
                            ThemeColors::TEXT_PRIMARY
                        }),
                        |ui| self.show_photo_filter(ui),
                    );
                    if popup.response.clicked()
                        && !self.photo_browser.completed
                        && self.photo_browser.request.is_none()
                        && self.photo_browser.revision != Some(self.playlist.catalog_revision())
                    {
                        self.request_photo_dates();
                    }
                    if active && ui.small_button("清除").clicked() {
                        self.apply_photo_filter(DateFilter::All);
                    }
                    ui.label(format!(
                        "{} / {} 張",
                        self.playlist.len(),
                        self.playlist.all_files().len()
                    ));
                    if self.photo_browser.request.is_some() {
                        ui.add(egui::Spinner::new().size(16.0))
                            .on_hover_text(format!(
                                "讀取日期 {}/{}；可由日期篩選面板取消",
                                self.photo_browser
                                    .index
                                    .progress()
                                    .min(self.playlist.all_files().len()),
                                self.playlist.all_files().len()
                            ));
                    }
                });
            });
    }

    fn show_photo_filter(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(260.0);
        ui.label("拍攝日期篩選");
        ui.horizontal(|ui| {
            for (filter, label) in [
                (DateFilter::All, "全部日期"),
                (DateFilter::Undated, "無可用日期"),
            ] {
                let enabled = filter == DateFilter::All
                    || (self.playlist.dates_indexed() && self.photo_browser.request.is_none());
                if ui
                    .add_enabled(
                        enabled,
                        egui::SelectableLabel::new(self.playlist.date_filter() == &filter, label),
                    )
                    .on_disabled_hover_text("請先完成日期讀取，再選擇無可用日期")
                    .clicked()
                {
                    self.apply_photo_filter(filter);
                    ui.close_menu();
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.photo_browser.day)
                    .hint_text("YYYY-MM-DD")
                    .desired_width(130.0)
                    .char_limit(10),
            );
            if ui.button("套用日期").clicked()
                || (edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                self.photo_browser.error = !valid_day(&self.photo_browser.day);
                if !self.photo_browser.error {
                    self.apply_photo_filter(DateFilter::Day(self.photo_browser.day.clone()));
                    ui.close_menu();
                }
            }
        });
        if self.photo_browser.error {
            ui.colored_label(ThemeColors::WARNING, "請輸入有效日期，例如 2025-01-02");
        }
        ui.label("或選擇相簿中的日期");
        let mut chosen_day = None;
        egui::ScrollArea::vertical()
            .id_salt("photo_filter_days")
            .max_height(200.0)
            .show_rows(ui, 26.0, self.photo_browser.days.len(), |ui, rows| {
                for row in rows {
                    let day = &self.photo_browser.days[row];
                    if ui
                        .selectable_label(
                            self.playlist.date_filter() == &DateFilter::Day(day.clone()),
                            day,
                        )
                        .clicked()
                    {
                        chosen_day = Some(day.clone());
                    }
                }
            });
        if let Some(day) = chosen_day {
            self.photo_browser.day = day.clone();
            self.photo_browser.error = false;
            self.apply_photo_filter(DateFilter::Day(day));
            ui.close_menu();
        }
        if self.photo_browser.days.is_empty() {
            ui.weak("尚無可用日期");
        }
        ui.separator();
        if self.photo_browser.request.is_some() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "讀取日期 {}/{}",
                    self.photo_browser
                        .index
                        .progress()
                        .min(self.playlist.all_files().len()),
                    self.playlist.all_files().len()
                ));
                if ui.small_button("取消讀取").clicked() {
                    self.photo_browser.index.cancel();
                    self.photo_browser.request = None;
                }
            });
        } else {
            if self.photo_browser.completed {
                ui.weak("日期索引已完成");
            }
            if ui.button("重新讀取日期").clicked() {
                self.request_photo_dates();
            }
        }
        ui.weak("以 EXIF 拍攝日期為準，不使用檔案修改日期")
            .on_hover_text(
                "僅讀取前 512 KiB 檔頭；使用相機當地時間，缺少或範圍外的 EXIF 列為無可用日期。",
            );
    }

    pub(crate) fn show_photo_grid(&mut self, ui: &mut egui::Ui) {
        if self.playlist.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(if self.playlist.all_files().is_empty() {
                    "請開啟圖片或資料夾"
                } else {
                    "目前篩選沒有圖片；可切回全部日期"
                });
            });
            return;
        }
        let columns = (ui.available_width() / 154.0).floor().max(1.0) as usize;
        let width = (ui.available_width() / columns as f32 - ui.spacing().item_spacing.x).max(1.0);
        let row_height = 168.0;
        let mut selected = None;
        egui::ScrollArea::vertical()
            .id_salt(("photo_grid", self.playlist.catalog_revision()))
            .auto_shrink([false, false])
            .show_rows(
                ui,
                row_height,
                self.playlist.len().div_ceil(columns),
                |ui, rows| {
                    for row in rows {
                        ui.horizontal(|ui| {
                            for index in
                                row * columns..((row + 1) * columns).min(self.playlist.len())
                            {
                                let path = &self.playlist.files()[index];
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(width, row_height),
                                    egui::Sense::click(),
                                );
                                let current = self.session.as_ref().and_then(|s| s.source_path())
                                    == Some(path.as_path());
                                ui.painter().rect_filled(
                                    rect.shrink(2.0),
                                    4.0,
                                    if current {
                                        ThemeColors::BG_WIDGET_ACTIVE
                                    } else {
                                        ThemeColors::BG_WIDGET
                                    },
                                );
                                if current || response.hovered() {
                                    ui.painter().rect_stroke(
                                        rect.shrink(2.0),
                                        4.0,
                                        egui::Stroke::new(1.5_f32, ThemeColors::ACCENT),
                                    );
                                }
                                let image_rect = Rect::from_min_max(
                                    rect.min + vec2(8.0, 8.0),
                                    pos2(rect.max.x - 8.0, rect.max.y - 44.0),
                                );
                                if ui.is_rect_visible(rect)
                                    && let Some(texture) =
                                        self.thumbnail_manager.get_or_request(path)
                                {
                                    let size = texture.size_vec2();
                                    let scale = (image_rect.width() / size.x)
                                        .min(image_rect.height() / size.y);
                                    ui.painter().image(
                                        texture.id(),
                                        Rect::from_center_size(image_rect.center(), size * scale),
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        egui::Color32::WHITE,
                                    );
                                }
                                let filename =
                                    path.file_name().unwrap_or_default().to_string_lossy();
                                let mut job = egui::text::LayoutJob::simple(
                                    filename.into_owned(),
                                    FontId::proportional(12.0),
                                    ThemeColors::TEXT_PRIMARY,
                                    width - 16.0,
                                );
                                job.wrap.max_rows = 1;
                                let galley = ui.fonts(|fonts| fonts.layout_job(job));
                                // 限制文字的裁切範圍，長檔名不侵入日期列。
                                ui.painter()
                                    .with_clip_rect(Rect::from_min_size(
                                        pos2(rect.min.x + 8.0, rect.max.y - 39.0),
                                        vec2(width - 16.0, 17.0),
                                    ))
                                    .galley(
                                        pos2(rect.min.x + 8.0, rect.max.y - 39.0),
                                        galley,
                                        ThemeColors::TEXT_PRIMARY,
                                    );
                                let date = self.playlist.captured_at(path).unwrap_or(
                                    if self.photo_browser.completed {
                                        "無可用拍攝日期"
                                    } else {
                                        "日期尚未讀取"
                                    },
                                );
                                ui.painter().text(
                                    pos2(rect.min.x + 8.0, rect.max.y - 19.0),
                                    Align2::LEFT_TOP,
                                    date.get(..10)
                                        .filter(|_| {
                                            date.as_bytes().first().is_some_and(u8::is_ascii_digit)
                                        })
                                        .unwrap_or(date),
                                    FontId::proportional(11.0),
                                    ThemeColors::TEXT_SECONDARY,
                                );
                                let clicked = response.clicked();
                                response.on_hover_text(format!("{}\n{}", path.display(), date));
                                if clicked {
                                    selected = Some(index);
                                }
                            }
                        });
                    }
                },
            );
        if let Some(index) = selected {
            if self.session.as_ref().and_then(|s| s.source_path())
                == Some(self.playlist.files()[index].as_path())
            {
                self.photo_browser.grid_open = false;
            } else {
                self.navigate_to(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn filtered_app() -> (
        tempfile::TempDir,
        ImageViewerApp,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("一.png");
        let b = dir.path().join("二.png");
        for path in [&a, &b] {
            image::RgbaImage::new(2, 3).save(path).unwrap();
        }
        let mut app = ImageViewerApp::default();
        app.open_file_path(a.clone()).unwrap();
        assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
        app.playlist
            .set_captured_dates(std::collections::HashMap::from([
                (a.clone(), "2025-01-01 10:00:00".into()),
                (b.clone(), "2025-01-02 10:00:00".into()),
            ]));
        app.apply_photo_filter(DateFilter::Day("2025-01-01".into()));
        (dir, app, a, b)
    }
    #[test]
    fn 明確開啟同目錄被篩除的圖片不重掃且清除日期條件() {
        let (_dir, mut app, _a, b) = filtered_app();
        let revision = app.playlist.catalog_revision();
        assert!(!app.playlist.files().contains(&b));
        // 索引仍在執行時明確開圖，舊結果也不能重新套用已清除的條件。
        app.request_photo_dates();
        app.open_file_path(b.clone()).unwrap();
        assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
        assert_eq!(
            app.playlist.catalog_revision(),
            revision,
            "已知來源不得重掃目錄"
        );
        assert_eq!(app.playlist.current_file(), Some(&b));
        assert_eq!(app.playlist.len(), 2);
        let ctx = egui::Context::default();
        let start = std::time::Instant::now();
        loop {
            app.poll_photo_browser(&ctx);
            if app.photo_browser.request.is_none() {
                break;
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(5));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(app.playlist.date_filter(), &DateFilter::All);
        assert_eq!(app.playlist.current_file(), Some(&b));
        assert_eq!(app.playlist.len(), 2);
        let output = ctx.run(Default::default(), |ctx| app.show_photo_toolbar(ctx));
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "日期篩選"))
        );
    }
    #[test]
    fn 重掃與重載保留條件且索引完成前不洩出被篩除圖片() {
        for reload in [false, true] {
            let (_dir, mut app, a, b) = filtered_app();
            app.refresh_directory(reload);
            let start = std::time::Instant::now();
            let ctx = egui::Context::default();
            loop {
                app.poll_file_actions();
                app.poll_document_io();
                assert!(
                    !app.playlist.files().contains(&b),
                    "重掃與索引之間不得暫時顯示全部圖片"
                );
                app.poll_photo_browser(&ctx);
                if !app.is_scanning_directory()
                    && !app.is_loading_image()
                    && app.photo_browser.request.is_none()
                {
                    break;
                }
                assert!(start.elapsed() < std::time::Duration::from_secs(5));
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert_eq!(
                app.session.as_ref().unwrap().source_path(),
                Some(a.as_path())
            );
            // 測資為不含 EXIF 的 PNG；重讀後指定日期應沒有結果。
            assert!(app.playlist.is_empty());
            assert_eq!(
                app.playlist.date_filter(),
                &DateFilter::Day("2025-01-01".into())
            );
        }
    }
    #[test]
    fn 日期篩選拒絕不存在的日期與非日期文字() {
        for day in ["2024-02-29", "2025-12-31", "2000-02-29"] {
            assert!(valid_day(day));
        }
        for day in [
            "2025-02-29",
            "1900-02-29",
            "0000-01-01",
            "2025-13-01",
            "2025-01-00",
            "２０２５-01-01",
            "2025-1-01",
        ] {
            assert!(!valid_day(day));
        }
    }

    #[test]
    fn 日期完成與清除篩選不改像素草稿或視口() {
        let mut app = ImageViewerApp::default();
        app.playlist
            .set_custom_list(vec!["相片.jpg".into(), "另一張.jpg".into()]);
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(2, 3),
            Some("相片.jpg".into()),
        ));
        app.slider_params.brightness = 1.2;
        app.viewport.zoom = 2.5;
        let pixels = app.session.as_ref().unwrap().active_image().clone();
        app.playlist
            .set_captured_dates(std::collections::HashMap::from([(
                "相片.jpg".into(),
                "2025-01-02 10:00:00".into(),
            )]));
        app.apply_photo_filter(DateFilter::Day("2024-01-01".into()));
        assert!(app.playlist.is_empty());
        assert!(app.has_unsaved_changes());
        app.apply_photo_filter(DateFilter::All);
        assert_eq!(app.playlist.current_file(), Some(&"相片.jpg".into()));
        assert_eq!(
            app.session.as_ref().unwrap().active_image().as_ref(),
            pixels.as_ref()
        );
        assert_eq!(app.viewport.zoom, 2.5);
        assert_eq!(app.slider_params.brightness, 1.2);
    }

    #[test]
    fn 背景開圖交付保留同輪最新日期與篩選() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("一.png");
        let b = dir.path().join("二.png");
        for path in [&a, &b] {
            image::RgbaImage::new(2, 3).save(path).unwrap();
        }
        let mut app = ImageViewerApp::default();
        app.playlist.set_custom_list(vec![a.clone(), b.clone()]);
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(2, 3),
            Some(a),
        ));
        app.open_file_path(b.clone()).unwrap();
        app.playlist
            .set_captured_dates(std::collections::HashMap::from([(
                b.clone(),
                "2025-01-02 10:00:00".into(),
            )]));
        app.apply_photo_filter(DateFilter::Day("2025-01-02".into()));
        assert!(app.wait_document_io(std::time::Duration::from_secs(5)));
        assert_eq!(app.playlist.files(), std::slice::from_ref(&b));
        assert_eq!(app.playlist.captured_at(&b), Some("2025-01-02 10:00:00"));
        assert_eq!(
            app.session.as_ref().unwrap().source_path(),
            Some(b.as_path())
        );
    }

    #[test]
    fn 大清單只繪製可見網格且點圖遵循取消離開保護() {
        let ctx = egui::Context::default();
        let mut app = ImageViewerApp::default();
        let paths: Vec<_> = (0..20_000)
            .map(|i| std::path::PathBuf::from(format!("相片{i}.png")))
            .collect();
        app.playlist.set_custom_list(paths.clone());
        app.photo_browser.grid_open = true;
        app.session = Some(viewer_state::EditingSession::new(
            image::RgbaImage::new(2, 3),
            Some(paths[0].clone()),
        ));
        app.slider_params.brightness = 1.2;
        let draw = |app: &mut ImageViewerApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.show_photo_grid(ui));
                },
            )
        };
        let output = draw(&mut app, vec![]);
        assert!(output.shapes.len() < 300, "虛擬化不得繪製整個清單");
        assert!(
            app.thumbnail_manager.pending_count() <= 25,
            "只請求可見列，不能預載整個相簿"
        );
        for pressed in [true, false] {
            draw(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos2(230.0, 80.0)),
                    egui::Event::PointerButton {
                        pos: pos2(230.0, 80.0),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(app.has_pending_leave());
        app.resolve_pending_leave(crate::navigation_guard::LeaveChoice::Cancel)
            .unwrap();
        assert!(app.photo_browser.grid_open);
        assert_eq!(
            app.session.as_ref().unwrap().source_path(),
            Some(paths[0].as_path())
        );
        assert!(app.has_unsaved_changes());
    }

    #[test]
    fn 索引進度與訊息不改工具列下方畫布高度() {
        let ctx = egui::Context::default();
        crate::theme::apply_theme(&ctx);
        let mut app = ImageViewerApp::default();
        let mut heights = Vec::new();
        for (complete, error) in [(false, false), (true, false), (true, true)] {
            app.photo_browser.completed = complete;
            app.photo_browser.error = error;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 700.0))),
                    ..Default::default()
                },
                |ctx| {
                    app.show_photo_toolbar(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| {
                        heights.push(ui.available_height());
                    });
                },
            );
        }
        assert!(heights.iter().all(|h| (*h - heights[0]).abs() < 0.01));
    }

    #[test]
    fn 日期彈出面板選取與清除不擠動畫布() {
        fn text_position(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
        }
        for dpi in [1.0, 2.0] {
            let ctx = egui::Context::default();
            crate::theme::apply_theme(&ctx);
            ctx.set_pixels_per_point(dpi);
            // DPI 變更須先完成一幀，才以新的邏輯視窗尺寸核對版面。
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                },
                |_| {},
            );
            let mut app = ImageViewerApp::default();
            app.photo_browser.completed = true;
            app.photo_browser.days = vec!["2025-01-02".into()];
            let mut heights = Vec::new();
            let mut draw = |app: &mut ImageViewerApp, events| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        app.show_photo_toolbar(ctx);
                        egui::CentralPanel::default().show(ctx, |ui| {
                            heights.push(ui.available_height());
                        });
                    },
                )
            };
            let _ = draw(&mut app, vec![]);
            for label in ["日期篩選", "2025-01-02", "清除"] {
                // 彈出面板出現首幀可能只進行尺寸量測，下一幀才有文字形狀。
                let output = draw(&mut app, vec![]);
                let pos =
                    text_position(&output, label).unwrap_or_else(|| panic!("找不到控制：{label}"));
                assert!(pos.x < 800.0, "最小視窗仍可操作日期篩選");
                for pressed in [true, false] {
                    let _ = draw(
                        &mut app,
                        vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            },
                        ],
                    );
                }
                if label == "2025-01-02" {
                    assert_eq!(*app.playlist.date_filter(), DateFilter::Day(label.into()));
                }
            }
            assert_eq!(*app.playlist.date_filter(), DateFilter::All);
            assert!(heights[0] > 540.0, "單列工具列必須釋出原本第二列的空間");
            assert!(
                heights.iter().all(|h| (*h - heights[0]).abs() < 0.01),
                "畫布高度變動：{heights:?}"
            );
        }
    }
}
