//! 圖片解碼、目錄掃描與原子寫檔的背景工作及交付狀態。

use crate::{app::ImageViewerApp, playlist::ImagePlaylist};
use core_archive::ManagedArchiveSession;
use image::RgbaImage;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub(crate) struct LoadedImage {
    pub opened_path: PathBuf,
    pub path: PathBuf,
    pub pixels: RgbaImage,
    pub playlist: ImagePlaylist,
    pub archive: Option<ManagedArchiveSession>,
    pub reference: bool,
    pub animated: bool,
    pub info: crate::convenience::FileInfo,
}

struct ReadTask {
    sort: crate::playlist::SortOptions,
    id: u64,
    path: PathBuf,
    playlist: ImagePlaylist,
    archive: Option<ManagedArchiveSession>,
    reference: bool,
}

enum Task {
    Read(Box<ReadTask>),
    Write {
        id: u64,
        path: PathBuf,
        image: Arc<RgbaImage>,
    },
}

enum Completion {
    ArchivePath {
        id: u64,
        path: PathBuf,
    },
    Read {
        id: u64,
        reference: bool,
        result: Result<Box<LoadedImage>, String>,
    },
    Write {
        id: u64,
        path: PathBuf,
        image: Arc<RgbaImage>,
        result: Result<Box<ImagePlaylist>, String>,
        info: Box<crate::convenience::FileInfo>,
    },
}

struct PendingRead {
    id: u64,
    path: PathBuf,
    origin: Option<Arc<RgbaImage>>,
    params: core_image::AdjustmentParams,
}

pub(crate) struct DocumentIo {
    sender: mpsc::Sender<Task>,
    receiver: mpsc::Receiver<Completion>,
    counter: u64,
    main_version: Arc<AtomicU64>,
    reference_version: Arc<AtomicU64>,
    pending_main: Option<PendingRead>,
    pending_reference: Option<u64>,
    pending_save: Option<u64>,
}

impl Default for DocumentIo {
    fn default() -> Self {
        let (sender, tasks) = mpsc::channel::<Task>();
        let (results, receiver) = mpsc::channel();
        let main_version = Arc::new(AtomicU64::new(0));
        let reference_version = Arc::new(AtomicU64::new(0));
        let main = Arc::clone(&main_version);
        let reference = Arc::clone(&reference_version);
        std::thread::spawn(move || {
            while let Ok(task) = tasks.recv() {
                let completion = match task {
                    Task::Read(task) => {
                        let mut task = *task;
                        let version = if task.reference { &reference } else { &main };
                        if version.load(Ordering::SeqCst) != task.id {
                            continue;
                        }
                        if !task.reference
                            && task.archive.is_none()
                            && task.path.extension().is_some_and(|extension| {
                                extension.eq_ignore_ascii_case("zip")
                                    || extension.eq_ignore_ascii_case("7z")
                            })
                            && !task.path.is_dir()
                        {
                            if results
                                .send(Completion::ArchivePath {
                                    id: task.id,
                                    path: task.path,
                                })
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                        let opened_path = task.archive.as_ref().map_or_else(
                            || task.path.clone(),
                            |archive| archive.archive_path.clone(),
                        );
                        let preparation: Result<(), core_image::ImageError> =
                            (|| {
                                task.playlist.set_sort(task.sort);
                                if task.path.is_dir() {
                                    task.playlist.scan_directory(&task.path)?;
                                    task.path =
                                        task.playlist.current_file().cloned().ok_or_else(|| {
                                            std::io::Error::other("資料夾內沒有支援的圖片")
                                        })?;
                                } else if task.archive.is_some() {
                                    task.playlist.refresh_metadata();
                                    task.path = task.playlist.files().first().cloned().ok_or_else(
                                        || std::io::Error::other("壓縮檔內沒有支援的圖片"),
                                    )?;
                                }
                                Ok(())
                            })();
                        let result = preparation
                            .and_then(|()| core_image::io::inspect_animation(&task.path))
                            .and_then(|animated| {
                                core_image::load_image(&task.path).map(|image| (image, animated))
                            })
                            .map(|(image, animated)| {
                                // 解碼無法中途停止；過期後不再掃描目錄或交付大型影像。
                                if version.load(Ordering::SeqCst) != task.id {
                                    return None;
                                }
                                if !task.reference
                                    && !task.playlist.try_select_existing(&task.path)
                                    && !task.playlist.contains_source(&task.path)
                                {
                                    task.playlist.scan_from_file(&task.path);
                                }
                                let info = crate::convenience::FileInfo::read(&task.path);
                                Some(LoadedImage {
                                    info,
                                    opened_path: opened_path.canonicalize().unwrap_or(opened_path),
                                    path: task.path,
                                    pixels: image.to_rgba8(),
                                    playlist: task.playlist,
                                    archive: task.archive,
                                    reference: task.reference,
                                    animated,
                                })
                            })
                            .map_err(|error| error.to_string());
                        let result = match result {
                            Ok(Some(loaded)) => Ok(loaded),
                            Ok(None) => continue,
                            Err(error) => Err(error),
                        };
                        Completion::Read {
                            id: task.id,
                            reference: task.reference,
                            result: result.map(Box::new),
                        }
                    }
                    Task::Write { id, path, image } => {
                        let result = core_image::save_image_atomic(
                            &image::DynamicImage::ImageRgba8((*image).clone()),
                            &path,
                        )
                        .map(|()| {
                            let mut playlist = ImagePlaylist::new();
                            playlist.scan_from_file(&path);
                            playlist
                        })
                        .map_err(|error| error.to_string());
                        Completion::Write {
                            id,
                            info: Box::new(crate::convenience::FileInfo::read(&path)),
                            path,
                            image,
                            result: result.map(Box::new),
                        }
                    }
                };
                if results.send(completion).is_err() {
                    break;
                }
            }
        });
        Self {
            sender,
            receiver,
            counter: 0,
            main_version,
            reference_version,
            pending_main: None,
            pending_reference: None,
            pending_save: None,
        }
    }
}

impl DocumentIo {
    pub fn intended_index(&self, playlist: &ImagePlaylist) -> Option<usize> {
        self.pending_main
            .as_ref()
            .and_then(|pending| {
                playlist
                    .files()
                    .iter()
                    .position(|path| path == &pending.path)
            })
            .or_else(|| playlist.current_index())
    }
    pub fn cancel_reference(&mut self) {
        self.reference_version.store(0, Ordering::SeqCst);
        self.pending_reference = None;
    }
    pub fn cancel_main(&mut self) {
        self.main_version.store(0, Ordering::SeqCst);
        self.pending_main = None;
    }

    fn next_id(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }
}

impl ImageViewerApp {
    pub fn is_saving(&self) -> bool {
        self.document_io.pending_save.is_some() || self.animation_save.is_some()
    }
    pub fn is_document_busy(&self) -> bool {
        self.is_saving() || self.is_transforming() || self.is_recycling() || self.export.is_busy()
    }
    pub fn is_loading_image(&self) -> bool {
        self.document_io.pending_main.is_some()
    }

    pub(crate) fn enqueue_image(
        &mut self,
        path: PathBuf,
        archive: Option<ManagedArchiveSession>,
        reference: bool,
    ) -> Result<(), String> {
        if self.is_document_busy() {
            return Err("儲存進行中，請稍候再開啟圖片".into());
        }
        let id = self.document_io.next_id();
        let mut playlist = self.playlist.clone();
        if let Some(archive) = &archive {
            playlist.set_custom_list(archive.image_paths.clone());
        }
        if reference {
            self.document_io
                .reference_version
                .store(id, Ordering::SeqCst);
            self.document_io.pending_reference = Some(id);
        } else {
            self.document_io.main_version.store(id, Ordering::SeqCst);
            self.document_io.pending_main = Some(PendingRead {
                id,
                path: path.clone(),
                origin: self.session.as_ref().map(|s| s.active_image()),
                params: self.slider_params,
            });
        }
        if self
            .document_io
            .sender
            .send(Task::Read(Box::new(ReadTask {
                sort: self.preferences.current.sort,
                id,
                path,
                playlist,
                archive,
                reference,
            })))
            .is_err()
        {
            if reference {
                self.document_io.pending_reference = None;
            } else {
                self.document_io.cancel_main();
            }
            return Err("圖片背景工作已停止".into());
        }
        self.status_notification = Some(("正在背景載入圖片…".into(), Instant::now()));
        Ok(())
    }

    pub(crate) fn enqueue_save(&mut self, path: PathBuf) -> bool {
        if self.is_document_busy() || self.is_loading_image() {
            return false;
        }
        let Some(image) = self.session.as_ref().map(|s| s.committed_image()) else {
            return false;
        };
        let id = self.document_io.next_id();
        if self
            .document_io
            .sender
            .send(Task::Write { id, path, image })
            .is_err()
        {
            return false;
        }
        self.document_io.pending_save = Some(id);
        self.status_notification = Some(("正在背景儲存圖片…".into(), Instant::now()));
        true
    }

    /// 非阻塞交付；回傳的成功或錯誤只影響目前仍有效的請求。
    pub fn poll_document_io(&mut self) {
        while let Ok(completion) = self.document_io.receiver.try_recv() {
            match completion {
                Completion::ArchivePath { id, path } => {
                    if !self
                        .document_io
                        .pending_main
                        .as_ref()
                        .is_some_and(|pending| pending.id == id)
                    {
                        continue;
                    }
                    let Some(pending) = self.document_io.pending_main.take() else {
                        continue;
                    };
                    let changed = self.slider_params != pending.params
                        || match (pending.origin, self.session.as_ref()) {
                            (Some(origin), Some(session)) => {
                                !Arc::ptr_eq(&origin, &session.active_image())
                            }
                            (None, None) => false,
                            _ => true,
                        };
                    if changed && self.has_unsaved_changes() {
                        self.defer_leave(crate::navigation_guard::LeaveAction::ArchivePath(path));
                    } else {
                        self.start_archive_path(path);
                    }
                }
                Completion::Read {
                    id,
                    reference,
                    result,
                } => {
                    let valid = if reference {
                        self.document_io.pending_reference == Some(id)
                    } else {
                        self.document_io
                            .pending_main
                            .as_ref()
                            .is_some_and(|p| p.id == id)
                    };
                    if !valid {
                        continue;
                    }
                    let attempted_path = if reference {
                        None
                    } else {
                        self.document_io
                            .pending_main
                            .as_ref()
                            .map(|pending| pending.path.clone())
                    };
                    let changed = if reference {
                        self.document_io.pending_reference = None;
                        false
                    } else {
                        let pending = self.document_io.pending_main.take();
                        pending.is_some_and(|p| {
                            self.slider_params != p.params
                                || match (p.origin, self.session.as_ref()) {
                                    (Some(origin), Some(session)) => {
                                        !Arc::ptr_eq(&origin, &session.active_image())
                                    }
                                    (None, None) => false,
                                    _ => true,
                                }
                        })
                    };
                    if !reference {
                        self.slideshow_loaded(result.is_ok(), Instant::now());
                    }
                    match result {
                        Ok(loaded) if !reference && changed && self.has_unsaved_changes() => {
                            self.defer_leave(crate::navigation_guard::LeaveAction::Loaded(loaded));
                        }
                        Ok(loaded) => self.install_loaded_image(*loaded),
                        Err(error) => {
                            if let Some(path) = attempted_path {
                                self.remove_recent(path);
                            }
                            let message = if self.slideshow.playing {
                                format!(
                                    "載入失敗：{error}；幻燈片已略過 {} 張",
                                    self.slideshow.failures
                                )
                            } else {
                                format!("載入失敗：{error}")
                            };
                            self.status_notification = Some((message, Instant::now()))
                        }
                    }
                }
                Completion::Write {
                    id,
                    path,
                    image,
                    result,
                    info,
                } => {
                    if self.document_io.pending_save != Some(id) {
                        continue;
                    }
                    self.document_io.pending_save = None;
                    match result {
                        Ok(mut playlist) => {
                            playlist.set_sort(self.preferences.current.sort);
                            let matches_current = self.session.as_ref().is_some_and(|session| {
                                Arc::ptr_eq(&image, &session.committed_image())
                            });
                            if let Some(session) = &mut self.session
                                && matches_current
                            {
                                session.set_source_path(path);
                                session.mark_saved();
                                self.animated_source = false;
                                self.convenience.info = *info;
                                self.playlist = *playlist;
                            }
                            if !matches_current {
                                self.leave_guard.save_then_leave = false;
                                self.leave_guard.error =
                                    Some("快照已儲存，但目前內容已變更，已取消離開。".into());
                                self.status_notification =
                                    Some(("快照已儲存，目前變更仍未儲存".into(), Instant::now()));
                                continue;
                            }
                            self.status_notification = Some(("圖片已儲存".into(), Instant::now()));
                            if self.leave_guard.save_then_leave {
                                self.leave_guard.save_then_leave = false;
                                let _ = self.finish_pending_leave();
                            }
                        }
                        Err(error) => {
                            self.leave_guard.save_then_leave = false;
                            self.leave_guard.error =
                                Some(format!("儲存失敗，已保留目前工作：{error}"));
                            self.status_notification =
                                Some((format!("儲存失敗：{error}"), Instant::now()));
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn io_needs_repaint(&self) -> bool {
        self.is_loading_image()
            || self.document_io.pending_reference.is_some()
            || self.is_document_busy()
    }

    /// 僅供測試及非介面同步流程使用；畫面更新只呼叫非阻塞輪詢。
    pub fn wait_document_io(&mut self, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            self.poll_export();
            self.poll_convenience();
            self.poll_file_actions();
            self.poll_document_io();
            if !self.io_needs_repaint() {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
