//! 重複圖片檢查器對話框面板 (Duplicate Finder Dialog)
//! 提供多執行緒 MD5/pHash 背景掃描、Union-Find 分群、左右雙圖預覽與安全刪除至回收桶

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use egui::{
    pos2, vec2, Align, Align2, Button, Color32, Frame, Layout, Margin, ProgressBar, Rect,
    RichText, Rounding, ScrollArea, Stroke, Ui, Window,
};
use rayon::prelude::*;

use crate::theme::ThemeColors;
use duplicate_finder::{
    compute_md5, compute_phash, find_exact_md5_groups, find_similar_groups, DuplicateGroup,
};

/// 查重模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateMode {
    /// MD5 精確重複 (位元組等級完全相同)
    ExactMd5,
    /// pHash 感知相似 (經剪裁、壓縮或亮度輕微擾動仍可識別)
    SimilarPhash,
}

/// 背景掃描進度訊息
pub enum ScanMessage {
    Progress {
        current: usize,
        total: usize,
        stage: String,
    },
    Finished {
        groups: Vec<DuplicateGroup>,
        total_scanned: usize,
    },
    Failed(String),
}

/// 掃描狀態
pub enum ScanStatus {
    Idle,
    Scanning {
        current: usize,
        total: usize,
        stage: String,
    },
    Completed {
        total_scanned: usize,
    },
    Error(String),
}

/// 單個重複項目之 UI 顯示資料
#[derive(Clone, Debug)]
pub struct DuplicateItemUi {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub dimensions: Option<(u32, u32)>,
    pub selected_for_delete: bool,
}

/// 重複群組之 UI 顯示資料
#[derive(Clone, Debug)]
pub struct DuplicateGroupUi {
    pub id: usize,
    pub max_distance: u32,
    pub is_expanded: bool,
    pub items: Vec<DuplicateItemUi>,
}

/// 重複圖片檢查器狀態管理器
pub struct DuplicateFinderState {
    pub is_open: bool,
    pub target_dir: Option<PathBuf>,
    pub recursive: bool,
    pub mode: DuplicateMode,
    pub hamming_threshold: u32,

    pub status: ScanStatus,
    pub groups: Vec<DuplicateGroupUi>,
    pub active_group_idx: Option<usize>,
    pub active_item_idx: Option<usize>,

    pub show_confirm_delete: bool,
    pub delete_status_msg: Option<(String, bool)>, // (訊息, 是否成功)

    rx_scan: Option<Receiver<ScanMessage>>,
    // 預覽縮圖快取
    preview_textures: HashMap<PathBuf, egui::TextureHandle>,
}

impl Default for DuplicateFinderState {
    fn default() -> Self {
        Self {
            is_open: false,
            target_dir: None,
            recursive: true,
            mode: DuplicateMode::SimilarPhash,
            hamming_threshold: 10,
            status: ScanStatus::Idle,
            groups: Vec::new(),
            active_group_idx: None,
            active_item_idx: None,
            show_confirm_delete: false,
            delete_status_msg: None,
            rx_scan: None,
            preview_textures: HashMap::new(),
        }
    }
}

impl DuplicateFinderState {
    /// 開啟檢查器並預設當前圖片所在目錄
    pub fn open_with_dir(&mut self, dir: Option<PathBuf>) {
        if self.target_dir.is_none() && dir.is_some() {
            self.target_dir = dir;
        }
        self.is_open = true;
    }

    /// 開始背景平行掃描
    pub fn start_scan(&mut self) {
        let Some(dir) = self.target_dir.clone() else {
            self.status = ScanStatus::Error("請先選擇要掃描的目錄".to_string());
            return;
        };

        if !dir.is_dir() {
            self.status = ScanStatus::Error("所選路徑不是有效的資料夾".to_string());
            return;
        }

        let (tx, rx) = channel();
        self.rx_scan = Some(rx);
        self.groups.clear();
        self.active_group_idx = None;
        self.active_item_idx = None;
        self.delete_status_msg = None;
        self.status = ScanStatus::Scanning {
            current: 0,
            total: 0,
            stage: "正在收集目錄檔案清單...".to_string(),
        };

        let recursive = self.recursive;
        let mode = self.mode;
        let threshold = self.hamming_threshold;

        thread::spawn(move || {
            // 1. 遞迴收集圖片檔案
            let mut files = Vec::new();
            collect_image_files(&dir, recursive, &mut files);
            let total_files = files.len();

            if total_files == 0 {
                let _ = tx.send(ScanMessage::Finished {
                    groups: Vec::new(),
                    total_scanned: 0,
                });
                return;
            }

            let _ = tx.send(ScanMessage::Progress {
                current: 0,
                total: total_files,
                stage: format!("發現 {} 個圖片檔案，準備計算特徵雜湊...", total_files),
            });

            // 2. 平行計算雜湊
            let completed_counter = Arc::new(AtomicUsize::new(0));

            let groups = match mode {
                DuplicateMode::ExactMd5 => {
                    let files_ref = &files;
                    let counter = Arc::clone(&completed_counter);
                    let tx_prog = tx.clone();

                    let md5_list: Vec<(PathBuf, String)> = files_ref
                        .par_iter()
                        .filter_map(|p| {
                            let hash = compute_md5(p).ok()?;
                            let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
                            if done.is_multiple_of(10) || done == total_files {
                                let _ = tx_prog.send(ScanMessage::Progress {
                                    current: done,
                                    total: total_files,
                                    stage: format!("正在計算檔案 MD5 雜湊 ({}/{})", done, total_files),
                                });
                            }
                            Some((p.clone(), hash))
                        })
                        .collect();

                    let _ = tx.send(ScanMessage::Progress {
                        current: total_files,
                        total: total_files,
                        stage: "正在進行精確重複聚類分析...".to_string(),
                    });

                    find_exact_md5_groups(&md5_list)
                }
                DuplicateMode::SimilarPhash => {
                    let files_ref = &files;
                    let counter = Arc::clone(&completed_counter);
                    let tx_prog = tx.clone();

                    let phash_list: Vec<(PathBuf, Vec<u8>)> = files_ref
                        .par_iter()
                        .filter_map(|p| {
                            let hash = compute_phash(p, 8).ok()?;
                            let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
                            if done.is_multiple_of(5) || done == total_files {
                                let _ = tx_prog.send(ScanMessage::Progress {
                                    current: done,
                                    total: total_files,
                                    stage: format!("正在解碼並計算 pHash 感知雜湊 ({}/{})", done, total_files),
                                });
                            }
                            Some((p.clone(), hash))
                        })
                        .collect();

                    let _ = tx.send(ScanMessage::Progress {
                        current: total_files,
                        total: total_files,
                        stage: "正在進行 Union-Find 相似度分群...".to_string(),
                    });

                    find_similar_groups(&phash_list, threshold)
                }
            };

            let _ = tx.send(ScanMessage::Finished {
                groups,
                total_scanned: total_files,
            });
        });
    }

    /// 輪詢接收背景進度訊息
    pub fn update_progress(&mut self) {
        let mut finished_msg = None;
        if let Some(rx) = &self.rx_scan {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    ScanMessage::Progress {
                        current,
                        total,
                        stage,
                    } => {
                        self.status = ScanStatus::Scanning {
                            current,
                            total,
                            stage,
                        };
                    }
                    ScanMessage::Finished { .. } | ScanMessage::Failed(_) => {
                        finished_msg = Some(msg);
                    }
                }
            }
        }

        if let Some(msg) = finished_msg {
            self.rx_scan = None;
            match msg {
                ScanMessage::Finished {
                    groups,
                    total_scanned,
                } => {
                    // 轉為 UI 結構並擷取元數據（檔案大小）
                    self.groups = groups
                        .into_iter()
                        .enumerate()
                        .map(|(idx, g)| {
                            let items = g
                                .items
                                .into_iter()
                                .map(|p| {
                                    let size_bytes = std::fs::metadata(&p)
                                        .map(|m| m.len())
                                        .unwrap_or(0);
                                    let dims = image::image_dimensions(&p).ok();
                                    DuplicateItemUi {
                                        path: p,
                                        size_bytes,
                                        dimensions: dims,
                                        selected_for_delete: false,
                                    }
                                })
                                .collect();

                            DuplicateGroupUi {
                                id: idx + 1,
                                max_distance: g.max_distance,
                                is_expanded: true,
                                items,
                            }
                        })
                        .collect();

                    self.status = ScanStatus::Completed { total_scanned };
                    if !self.groups.is_empty() {
                        self.active_group_idx = Some(0);
                        if self.groups[0].items.len() > 1 {
                            self.active_item_idx = Some(1);
                        } else {
                            self.active_item_idx = Some(0);
                        }
                    }
                }
                ScanMessage::Failed(err) => {
                    self.status = ScanStatus::Error(err);
                }
                _ => {}
            }
        }
    }

    /// 批次智慧勾選：保留每組第一張，其餘標記刪除
    pub fn mark_all_duplicates_except_first(&mut self) {
        for group in &mut self.groups {
            for (idx, item) in group.items.iter_mut().enumerate() {
                item.selected_for_delete = idx > 0;
            }
        }
    }

    /// 批次智慧勾選：保留解析度最高者，其餘標記刪除
    pub fn mark_all_duplicates_keep_highest_resolution(&mut self) {
        for group in &mut self.groups {
            if group.items.is_empty() {
                continue;
            }
            // 找出像素最多的索引
            let mut best_idx = 0;
            let mut max_pixels = 0u64;

            for (idx, item) in group.items.iter().enumerate() {
                let pixels = if let Some((w, h)) = item.dimensions {
                    (w as u64) * (h as u64)
                } else {
                    item.size_bytes
                };
                if pixels > max_pixels {
                    max_pixels = pixels;
                    best_idx = idx;
                }
            }

            for (idx, item) in group.items.iter_mut().enumerate() {
                item.selected_for_delete = idx != best_idx;
            }
        }
    }

    /// 全部取消勾選
    pub fn uncheck_all(&mut self) {
        for group in &mut self.groups {
            for item in &mut group.items {
                item.selected_for_delete = false;
            }
        }
    }

    /// 統計當前勾選欲刪除的檔案總數與大小
    pub fn selected_delete_stats(&self) -> (usize, u64) {
        let mut count = 0;
        let mut total_bytes = 0;
        for group in &self.groups {
            for item in &group.items {
                if item.selected_for_delete {
                    count += 1;
                    total_bytes += item.size_bytes;
                }
            }
        }
        (count, total_bytes)
    }

    /// 執行安全刪除至系統回收桶
    pub fn execute_safe_delete(&mut self) {
        let mut deleted_count = 0;
        let mut errors = Vec::new();

        for group in &mut self.groups {
            let mut remaining = Vec::new();
            for item in group.items.drain(..) {
                if item.selected_for_delete {
                    match trash::delete(&item.path) {
                        Ok(()) => {
                            deleted_count += 1;
                        }
                        Err(e) => {
                            errors.push(format!(
                                "{}: {}",
                                item.path.file_name().unwrap_or_default().to_string_lossy(),
                                e
                            ));
                            remaining.push(item);
                        }
                    }
                } else {
                    remaining.push(item);
                }
            }
            group.items = remaining;
        }

        // 清理成員小於 2 的空群組
        self.groups.retain(|g| g.items.len() >= 2);
        if let Some(active) = self.active_group_idx
            && active >= self.groups.len()
        {
            self.active_group_idx = if self.groups.is_empty() {
                None
            } else {
                Some(self.groups.len() - 1)
            };
        }

        if errors.is_empty() {
            self.delete_status_msg = Some((
                format!("已成功將 {} 個檔案移至系統資源回收桶！", deleted_count),
                true,
            ));
        } else {
            self.delete_status_msg = Some((
                format!(
                    "刪除完成：成功 {} 個，失敗 {} 個 ({})",
                    deleted_count,
                    errors.len(),
                    errors.join("; ")
                ),
                false,
            ));
        }

        self.show_confirm_delete = false;
    }

    /// 取得或載入縮圖紋理
    pub fn get_or_load_preview(
        &mut self,
        ctx: &egui::Context,
        path: &Path,
    ) -> Option<egui::TextureHandle> {
        if let Some(tex) = self.preview_textures.get(path) {
            return Some(tex.clone());
        }

        if let Ok(img) = image::open(path) {
            let thumb = img.thumbnail(256, 256);
            let rgba = thumb.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            let tex = ctx.load_texture(
                path.to_string_lossy().to_string(),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            self.preview_textures.insert(path.to_path_buf(), tex.clone());
            Some(tex)
        } else {
            None
        }
    }
}

/// 遞迴或非遞迴掃描圖片檔案
fn collect_image_files(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let supported_exts = ["jpg", "jpeg", "png", "bmp", "webp", "gif", "tiff", "tif"];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if supported_exts.contains(&ext_lower.as_str()) {
                        out.push(path);
                    }
                }
            } else if recursive && path.is_dir() {
                collect_image_files(&path, true, out);
            }
        }
    }
}

/// 重複圖片檢查器 GUI 面板渲染元件
pub struct DuplicateFinderDialog;

impl DuplicateFinderDialog {
    /// 渲染重複檢查器主視窗
    /// 回傳若點擊「在檢視器開啟」時目標圖片路徑
    pub fn show(
        ctx: &egui::Context,
        state: &mut DuplicateFinderState,
    ) -> Option<PathBuf> {
        if !state.is_open {
            return None;
        }

        state.update_progress();
        if matches!(state.status, ScanStatus::Scanning { .. }) {
            ctx.request_repaint();
        }

        let mut navigate_to_path = None;
        let is_scanning = matches!(state.status, ScanStatus::Scanning { .. });

        let screen_size = ctx.screen_rect().size();
        let max_dialog_height = (screen_size.y - 140.0).max(380.0);
        let default_width = (screen_size.x * 0.82).clamp(720.0, 1100.0);
        let default_height = (screen_size.y * 0.62).clamp(400.0, max_dialog_height);

        let mut is_open = state.is_open;

        Window::new("👥 重複圖片檢查器 (Duplicate Finder)")
            .open(&mut is_open)
            .default_size(vec2(default_width, default_height))
            .min_size(vec2(680.0, 360.0))
            .max_size(vec2(screen_size.x - 40.0, max_dialog_height))
            .resizable(true)
            .collapsible(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, -25.0))
            .frame(
                Frame::window(&ctx.style())
                    .fill(Color32::from_rgb(28, 30, 36))
                    .inner_margin(Margin::same(14.0))
                    .stroke(Stroke::new(1.5, Color32::from_rgb(75, 82, 98)))
                    .rounding(Rounding::same(10.0)),
            )
            .show(ctx, |ui| {
                // 1. 頂部控制與設定列
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📂 掃描目錄:").strong().color(Color32::from_rgb(245, 245, 250)));

                    let dir_display = state
                        .target_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "尚未選擇目錄...".to_string());

                    ui.label(
                        RichText::new(dir_display)
                            .color(if state.target_dir.is_some() {
                                Color32::from_rgb(147, 197, 253) // 明亮天藍色
                            } else {
                                Color32::from_rgb(160, 165, 180)
                            })
                            .monospace(),
                    );

                    if ui.add_enabled(!is_scanning, Button::new(RichText::new("瀏覽 (Browse)...").color(Color32::WHITE))).clicked()
                        && let Some(folder) = rfd::FileDialog::new().pick_folder()
                    {
                        state.target_dir = Some(folder);
                    }

                    ui.add_space(8.0);
                    ui.add_enabled(
                        !is_scanning,
                        egui::Checkbox::new(
                            &mut state.recursive,
                            RichText::new("包含子目錄").color(Color32::from_rgb(225, 225, 235)),
                        ),
                    );

                    ui.add_space(16.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // 模式切換
                    ui.label(RichText::new("模式:").strong().color(Color32::from_rgb(245, 245, 250)));
                    let phash_active = state.mode == DuplicateMode::SimilarPhash;
                    let phash_btn = Button::new(
                        RichText::new("🔍 感知相似 (pHash)")
                            .color(if phash_active { Color32::WHITE } else { Color32::from_rgb(205, 210, 225) }),
                    )
                    .fill(if phash_active { ThemeColors::ACCENT } else { Color32::from_rgb(45, 48, 58) });
                    if ui.add(phash_btn).clicked() {
                        state.mode = DuplicateMode::SimilarPhash;
                    }

                    let md5_active = state.mode == DuplicateMode::ExactMd5;
                    let md5_btn = Button::new(
                        RichText::new("⚡ 精確重複 (MD5)")
                            .color(if md5_active { Color32::WHITE } else { Color32::from_rgb(205, 210, 225) }),
                    )
                    .fill(if md5_active { ThemeColors::ACCENT } else { Color32::from_rgb(45, 48, 58) });
                    if ui.add(md5_btn).clicked() {
                        state.mode = DuplicateMode::ExactMd5;
                    }
                });

                ui.add_space(8.0);

                // 2. 門檻微調與操作按鈕列
                ui.horizontal(|ui| {
                    if state.mode == DuplicateMode::SimilarPhash {
                        ui.label(RichText::new("相似度門檻 (Hamming):").color(Color32::from_rgb(225, 225, 235)));
                        ui.add(egui::Slider::new(&mut state.hamming_threshold, 0..=30).text(RichText::new("差距 ≤ 門檻判定為重複").color(Color32::from_rgb(180, 185, 200))));
                        ui.add_space(14.0);
                    }

                    // 開始掃描按鈕
                    let scan_btn_text = if is_scanning { "⏳ 掃描中..." } else { "▶ 開始掃描" };
                    let scan_btn = Button::new(RichText::new(scan_btn_text).strong().color(Color32::WHITE))
                        .fill(if is_scanning { ThemeColors::BG_WIDGET } else { ThemeColors::ACCENT });

                    if ui.add_enabled(!is_scanning && state.target_dir.is_some(), scan_btn).clicked() {
                        state.start_scan();
                    }

                    // 狀態與訊息
                    if let Some((msg, success)) = &state.delete_status_msg {
                        ui.add_space(12.0);
                        let color = if *success { ThemeColors::SUCCESS } else { ThemeColors::WARNING };
                        ui.label(RichText::new(msg).color(color).strong());
                    }
                });

                // 3. 掃描進度條
                if let ScanStatus::Scanning { current, total, stage } = &state.status {
                    ui.add_space(8.0);
                    let progress = if *total > 0 {
                        *current as f32 / *total as f32
                    } else {
                        0.0
                    };
                    ui.add(
                        ProgressBar::new(progress)
                            .show_percentage()
                            .text(format!("{} ({} / {})", stage, current, total))
                            .animate(true),
                    );
                }

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                // 4. 重複結果操作便捷列 (當有結果時顯示)
                if !state.groups.is_empty() {
                    let (del_count, del_bytes) = state.selected_delete_stats();
                    let mb = del_bytes as f64 / (1024.0 * 1024.0);

                    ui.horizontal(|ui| {
                        let total_dups: usize = state.groups.iter().map(|g| g.items.len()).sum();
                        ui.label(
                            RichText::new(format!(
                                "共發現 {} 組重複（{} 個檔案）",
                                state.groups.len(),
                                total_dups
                            ))
                            .strong()
                            .size(14.0)
                            .color(Color32::WHITE),
                        );

                        ui.add_space(12.0);
                        if ui.button(RichText::new("✔ 保留每組首項 (勾選其餘)").color(Color32::from_rgb(240, 240, 245))).clicked() {
                            state.mark_all_duplicates_except_first();
                        }
                        if ui.button(RichText::new("⭐ 保留畫質最高 (其餘勾選)").color(Color32::from_rgb(240, 240, 245))).clicked() {
                            state.mark_all_duplicates_keep_highest_resolution();
                        }
                        if ui.button(RichText::new("✖ 清除勾選").color(Color32::from_rgb(240, 240, 245))).clicked() {
                            state.uncheck_all();
                        }

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let delete_btn = Button::new(
                                RichText::new(format!(
                                    "🗑 將勾選項移至回收桶 ({} 項, {:.1} MB)",
                                    del_count, mb
                                ))
                                .strong()
                                .color(Color32::WHITE),
                            )
                            .fill(if del_count > 0 {
                                Color32::from_rgb(220, 38, 38)
                            } else {
                                Color32::from_rgb(60, 60, 65)
                            });

                            if ui.add_enabled(del_count > 0, delete_btn).clicked() {
                                state.show_confirm_delete = true;
                            }
                        });
                    });

                    ui.add_space(6.0);
                    ui.separator();
                }

                // 5. 主內容區域：左側群組清單 + 右側左右雙圖比對預覽
                let available_height = (ui.available_height() - 8.0).max(200.0);
                ui.columns(2, |cols| {
                    // 左側欄：群組與檔案清單
                    cols[0].group(|ui| {
                        ui.set_max_height(available_height);
                        ui.heading(RichText::new("📋 重複群組清單").size(14.0).color(ThemeColors::TEXT_SECONDARY));
                        ui.add_space(4.0);

                        if state.groups.is_empty() {
                            if matches!(state.status, ScanStatus::Completed { .. }) {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(RichText::new("🎉 太棒了！未發現任何重複圖片。").size(15.0).color(ThemeColors::SUCCESS));
                                });
                            } else {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(RichText::new("請選擇資料夾並點擊「開始掃描」").color(ThemeColors::TEXT_MUTED));
                                });
                            }
                        } else {
                            ScrollArea::vertical()
                                .id_salt("duplicate_groups_scroll")
                                .max_height(available_height - 30.0)
                                .show(ui, |ui| {
                                    for g_idx in 0..state.groups.len() {
                                        let group = &state.groups[g_idx];
                                        let is_active_group = state.active_group_idx == Some(g_idx);

                                        let header_title = format!(
                                            "群組 #{} ({} 個項目, 最大距離: {})",
                                            group.id,
                                            group.items.len(),
                                            group.max_distance
                                        );

                                        let group_frame = Frame::none()
                                            .fill(if is_active_group {
                                                Color32::from_rgb(38, 44, 58)
                                            } else {
                                                Color32::from_rgb(34, 36, 44)
                                            })
                                            .stroke(Stroke::new(
                                                1.0,
                                                if is_active_group {
                                                    Color32::from_rgb(96, 165, 250)
                                                } else {
                                                    Color32::from_rgb(60, 65, 78)
                                                },
                                            ))
                                            .rounding(Rounding::same(6.0))
                                            .inner_margin(Margin::same(8.0));

                                        group_frame.show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let fold_icon = if state.groups[g_idx].is_expanded { "▼" } else { "▶" };
                                                if ui.button(RichText::new(fold_icon).size(12.0).color(Color32::WHITE)).clicked() {
                                                    state.groups[g_idx].is_expanded = !state.groups[g_idx].is_expanded;
                                                }

                                                let resp = ui.selectable_label(
                                                    is_active_group,
                                                    RichText::new(header_title).strong().color(Color32::WHITE),
                                                );
                                                if resp.clicked() {
                                                    state.active_group_idx = Some(g_idx);
                                                    if state.groups[g_idx].items.len() > 1 {
                                                        state.active_item_idx = Some(1);
                                                    } else {
                                                        state.active_item_idx = Some(0);
                                                    }
                                                }
                                            });

                                            if state.groups[g_idx].is_expanded {
                                                ui.add_space(4.0);
                                                let mut clicked_item_idx = None;
                                                for item_idx in 0..state.groups[g_idx].items.len() {
                                                    let is_selected_item = is_active_group && state.active_item_idx == Some(item_idx);
                                                    let item_mut = &mut state.groups[g_idx].items[item_idx];

                                                    let filename = item_mut
                                                        .path
                                                        .file_name()
                                                        .unwrap_or_default()
                                                        .to_string_lossy()
                                                        .to_string();

                                                    let size_str = format!("{:.1} KB", item_mut.size_bytes as f64 / 1024.0);
                                                    let dim_str = item_mut
                                                        .dimensions
                                                        .map(|(w, h)| format!("{}x{}", w, h))
                                                        .unwrap_or_else(|| "未知解析度".to_string());

                                                    let is_del = item_mut.selected_for_delete;
                                                    let label_text = format!(
                                                        "{} ({}, {})",
                                                        filename, dim_str, size_str
                                                    );

                                                    ui.horizontal(|ui| {
                                                        ui.add_space(16.0);
                                                        // 刪除勾選框
                                                        ui.checkbox(&mut item_mut.selected_for_delete, "");

                                                        let item_resp = ui.selectable_label(
                                                            is_selected_item,
                                                            RichText::new(label_text).color(
                                                                if is_del {
                                                                    Color32::from_rgb(248, 113, 113) // 明亮鮮紅
                                                                } else if is_selected_item {
                                                                    Color32::from_rgb(147, 197, 253) // 明亮藍
                                                                } else {
                                                                    Color32::from_rgb(240, 240, 245) // 高對比亮白
                                                                },
                                                            ),
                                                        );

                                                        if item_resp.clicked() {
                                                            clicked_item_idx = Some(item_idx);
                                                        }
                                                    });
                                                }
                                                if let Some(item_idx) = clicked_item_idx {
                                                    state.active_group_idx = Some(g_idx);
                                                    state.active_item_idx = Some(item_idx);
                                                }
                                            }
                                        });
                                        ui.add_space(6.0);
                                    }
                                });
                        }
                    });

                    // 先複製比對資料以避免與 state 可變借用衝突
                    let preview_pair = if let Some(g_idx) = state.active_group_idx {
                        if let Some(group) = state.groups.get(g_idx) {
                            if !group.items.is_empty() {
                                let ref_item = group.items[0].clone();
                                let comp_idx = match state.active_item_idx {
                                    Some(idx) if idx < group.items.len() => idx,
                                    _ => if group.items.len() > 1 { 1 } else { 0 },
                                };
                                let comp_item = group.items[comp_idx].clone();
                                Some((ref_item, comp_item, comp_idx))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    // 右側欄：雙圖左右比對預覽視圖
                    cols[1].group(|ui| {
                        ui.set_max_height(available_height);
                        ui.heading(RichText::new("🔍 左右比對預覽").size(14.0).color(ThemeColors::TEXT_SECONDARY));
                        ui.add_space(4.0);

                        ScrollArea::vertical()
                            .id_salt("duplicate_preview_scroll")
                            .max_height(available_height - 30.0)
                            .show(ui, |ui| {
                                if let Some((ref_item, comp_item, comp_idx)) = preview_pair {
                                    ui.columns(2, |sub_cols| {
                                        // 左：基準圖
                                        sub_cols[0].group(|ui| {
                                            ui.label(RichText::new("📌 基準圖 (項 1)").strong().color(ThemeColors::ACCENT));
                                            render_preview_card(
                                                ctx,
                                                ui,
                                                state,
                                                &ref_item,
                                                &mut navigate_to_path,
                                            );
                                        });

                                        // 右：比對圖
                                        sub_cols[1].group(|ui| {
                                            let title = if comp_idx == 0 {
                                                "🔍 當前圖 (同項 1)".to_string()
                                            } else {
                                                format!("🔍 比對圖 (項 {})", comp_idx + 1)
                                            };
                                            ui.label(RichText::new(title).strong().color(ThemeColors::ACCENT_HOVER));
                                            render_preview_card(
                                                ctx,
                                                ui,
                                                state,
                                                &comp_item,
                                                &mut navigate_to_path,
                                            );
                                        });
                                    });

                                    ui.add_space(8.0);
                                    ui.separator();
                                    ui.add_space(4.0);

                                    // 詳細路徑比對 (高對比清楚區分兩條路徑)
                                    ui.label(RichText::new("路徑詳細比對：").strong().color(Color32::WHITE));
                                    ui.label(RichText::new(format!("基準: {}", ref_item.path.display())).monospace().size(11.0).color(Color32::from_rgb(147, 197, 253)));
                                    ui.label(RichText::new(format!("比對: {}", comp_item.path.display())).monospace().size(11.0).color(Color32::from_rgb(253, 224, 71)));
                                } else {
                                    ui.vertical_centered(|ui| {
                                        ui.add_space(40.0);
                                        ui.label(RichText::new("點擊左側任一重複群組進行雙圖預覽比對").color(ThemeColors::TEXT_MUTED));
                                    });
                                }
                            });
                    });
                });
            });

        state.is_open = is_open;

        // 二次確認安全刪除對話框
        if state.show_confirm_delete {
            let (del_count, del_bytes) = state.selected_delete_stats();
            let mb = del_bytes as f64 / (1024.0 * 1024.0);

            Window::new("⚠️ 確認移至回收桶")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .fixed_size(vec2(380.0, 180.0))
                .frame(
                    Frame::window(&ctx.style())
                        .fill(ThemeColors::BG_PANEL)
                        .stroke(Stroke::new(1.0, Color32::from_rgb(220, 38, 38)))
                        .rounding(Rounding::same(8.0))
                        .inner_margin(Margin::same(16.0)),
                )
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("🗑 安全刪除確認").size(18.0).strong().color(Color32::from_rgb(239, 68, 68)));
                        ui.add_space(8.0);
                        ui.label(format!(
                            "您即將將 {} 個選取的重複檔案（共約 {:.2} MB）\n移至系統資源回收桶。",
                            del_count, mb
                        ));
                        ui.label(RichText::new("檔案不會永久消失，您隨時可從回收桶還原。").size(11.0).color(ThemeColors::TEXT_MUTED));
                        ui.add_space(14.0);

                        ui.horizontal(|ui| {
                            if ui.button("取消 (Cancel)").clicked() {
                                state.show_confirm_delete = false;
                            }
                            ui.add_space(20.0);
                            let confirm_btn = Button::new(RichText::new("確定移至回收桶").strong().color(Color32::WHITE))
                                .fill(Color32::from_rgb(220, 38, 38));
                            if ui.add(confirm_btn).clicked() {
                                state.execute_safe_delete();
                            }
                        });
                    });
                });
        }

        navigate_to_path
    }
}

/// 繪製單張圖片預覽卡片
fn render_preview_card(
    ctx: &egui::Context,
    ui: &mut Ui,
    state: &mut DuplicateFinderState,
    item: &DuplicateItemUi,
    navigate_to_path: &mut Option<PathBuf>,
) {
    let filename = item
        .path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    ui.label(RichText::new(filename).strong().color(Color32::WHITE));

    let dim_str = item
        .dimensions
        .map(|(w, h)| format!("{} x {}", w, h))
        .unwrap_or_else(|| "未知".to_string());
    let size_str = format!("{:.2} KB", item.size_bytes as f64 / 1024.0);

    ui.label(RichText::new(format!("解析度: {}", dim_str)).size(11.0).color(Color32::from_rgb(215, 220, 230)));
    ui.label(RichText::new(format!("大小: {}", size_str)).size(11.0).color(Color32::from_rgb(215, 220, 230)));

    ui.add_space(6.0);

    // 縮圖畫布容器 (高度適中 140.0，避免在小解析度下撐出視窗)
    let preview_size = vec2(ui.available_width().max(100.0), 140.0);
    let (rect, _) = ui.allocate_exact_size(preview_size, egui::Sense::hover());

    ui.painter().rect_filled(rect, Rounding::same(6.0), ThemeColors::BG_CANVAS);
    ui.painter().rect_stroke(rect, Rounding::same(6.0), Stroke::new(1.0, ThemeColors::BORDER));

    if let Some(tex) = state.get_or_load_preview(ctx, &item.path) {
        // 計算保持長寬比居中縮放
        let tex_size = tex.size_vec2();
        let scale = (rect.width() / tex_size.x).min(rect.height() / tex_size.y);
        let fitted_size = tex_size * scale;
        let img_rect = Rect::from_center_size(rect.center(), fitted_size);

        ui.painter().image(
            tex.id(),
            img_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    } else {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "🖼 無法預覽",
            egui::FontId::proportional(14.0),
            ThemeColors::TEXT_MUTED,
        );
    }

    ui.add_space(6.0);

    // 跳轉到主檢視器
    if ui.button("🖥 在主檢視器開啟 (Open)").clicked() {
        *navigate_to_path = Some(item.path.clone());
    }
}
