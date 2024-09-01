mod image_entry;
mod layout;
mod macros;
mod utils;
mod video_entry;
mod widgets;

use eframe::egui::{self, text, Visuals};
use ffmpeg_next as ffmpeg;
use futures::{executor, FutureExt};
use image_entry::ImageEntry;
use layout::{build_grid, build_preview};
use rayon::prelude::*;
use rfd::AsyncFileDialog;
use std::{
    env,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};
use utils::{
    filter_media_files, filter_valid_paths, is_image, process_entries, SharedTextureManager,
};
use video_entry::VideoEntry;

#[derive(PartialEq, Clone)]
pub enum MediaType {
    ImageStill,
    ImageAnimated,
    Video,
}

pub struct EguiWindow {
    pub title: String,
    pub open: bool,
}

#[derive(Default, Clone)]
pub struct Settings {
    pub min_thumbnail_size: usize,
    pub max_columns_count: usize,
    pub show_failed_files: bool,
}

pub struct GalleryEntry {
    path: PathBuf,
    thumbnail: egui::TextureHandle,
    media_type: MediaType,
    marked: bool,
    failed: bool,
}

pub struct CurrentEntry {
    media_type: MediaType,
    image: Option<ImageEntry>,
    video: Option<VideoEntry>,
}

impl Clone for GalleryEntry {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            thumbnail: self.thumbnail.clone(),
            media_type: self.media_type.clone(),
            marked: self.marked,
            failed: self.failed,
        }
    }
}

#[derive(Default)]
pub struct App {
    entries: Arc<Mutex<Vec<GalleryEntry>>>,
    current_entry: Option<CurrentEntry>,
    last_marked_entry_index: Option<usize>,
    dropped_files: Vec<PathBuf>,
    settings: Settings,
    windows: Vec<EguiWindow>,
}

impl App {
    fn new(_creation_ctx: &eframe::CreationContext<'_>, dropped_files: Vec<PathBuf>) -> Self {
        Self {
            settings: Settings {
                min_thumbnail_size: 200,
                max_columns_count: 4,
                show_failed_files: true,
            },
            dropped_files,
            ..Default::default()
        }
    }
}

fn load_files(
    files: Vec<PathBuf>,
    texture_manager: SharedTextureManager,
    entries: Arc<Mutex<Vec<GalleryEntry>>>,
    app_settings: Settings,
) {
    thread::spawn(move || {
        files.into_par_iter().for_each(move |file| {
            let max_thumbnail_size = 512.0;

            let texture = match ImageEntry::load_thumbnail(
                &texture_manager,
                &file,
                max_thumbnail_size,
            ) {
                Some(thumbnail) => thumbnail.get_texture(),
                None => None,
            };

            if app_settings.show_failed_files {
                entries.lock().unwrap().push(GalleryEntry {
                    path: file.clone(),
                    failed: texture.is_none(),
                    thumbnail: match texture {
                        Some(texture) => texture,
                        None => ImageEntry::default_texture(texture_manager.clone()),
                    },
                    media_type: if is_image(&file) {
                        MediaType::ImageStill
                    } else {
                        MediaType::Video
                    },
                    marked: false,
                });

                return;
            }

            match texture {
                Some(texture) => {
                    entries.lock().unwrap().push(GalleryEntry {
                        path: file.clone(),
                        thumbnail: texture,
                        media_type: if is_image(&file) {
                            MediaType::ImageStill
                        } else {
                            MediaType::Video
                        },
                        marked: false,
                        failed: false,
                    });
                }
                None => {
                    println!("Failed to load texture for file: {:?}", file);
                }
            }
        });
    });
}

fn handle_selector_button_click(ctx: egui::Context, app: &mut App, select_files: bool) {
    let file_dialog = AsyncFileDialog::new();
    let task = if select_files {
        file_dialog.pick_files().boxed()
    } else {
        file_dialog.pick_folders().boxed()
    };

    app.last_marked_entry_index = None;
    app.entries.lock().unwrap().clear();
    let entries = Arc::clone(&app.entries);
    let texture_manager = ctx.tex_manager();
    let settings = app.settings.clone();

    std::thread::spawn(move || {
        let result = executor::block_on(task);

        match result {
            Some(files) => {
                let files = files.iter().map(|file| PathBuf::from(file)).collect();
                let new_files = filter_media_files(process_entries(files));

                load_files(new_files, texture_manager, entries, settings);
            }
            None => {
                println!("No files selected");
            }
        }
    });
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(Visuals::dark());

        // Check if we have dropped files that we need to load
        if !self.dropped_files.is_empty() {
            let dropped_files = self.dropped_files.clone();
            let entries = Arc::clone(&self.entries);
            let texture_manager = ctx.tex_manager();
            let settings = self.settings.clone();

            self.dropped_files.clear();

            load_files(dropped_files, texture_manager, entries, settings);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.current_entry.is_some() {
                build_preview(self, &ctx, ui);
                return;
            }

            ui.vertical_centered_justified(|ui| {
                ui.horizontal(|ui| {
                    let files_selector_btn = ui.button("Select files");
                    ui.add_space(10.0);

                    let folders_selector_btn = ui.button("Select folders");
                    ui.add_space(10.0);

                    if files_selector_btn.clicked() {
                        handle_selector_button_click(ctx.clone(), self, true);
                    }

                    if folders_selector_btn.clicked() {
                        handle_selector_button_click(ctx.clone(), self, false);
                    }

                    let settings_btn = ui.button("Settings");

                    if settings_btn.clicked() {
                        self.windows.push(EguiWindow {
                            title: "Settings".to_string(),
                            open: true,
                        });
                    }

                    ui.add_space(10.0);

                    let number_of_images_label =
                        format!("Entries: {}", self.entries.lock().unwrap().len());
                    ui.label(number_of_images_label);
                });

                for window in self.windows.iter_mut() {
                    egui::Window::new(window.title.clone())
                        .open(&mut window.open)
                        .resizable(true)
                        .max_width(400.0)
                        .max_height(250.0)
                        .show(ui.ctx(), |ui| {
                            ui.add(
                                egui::Slider::new(&mut self.settings.min_thumbnail_size, 100..=512)
                                    .text("Min thumbnail size"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.settings.max_columns_count, 1..=10)
                                    .text("Max columns count"),
                            );

                            ui.checkbox(
                                &mut self.settings.show_failed_files,
                                "Show images that failed to load",
                            );

                            ui.allocate_space(ui.available_size());
                        });
                }

                ui.add_space(10.0);

                if !self.entries.lock().unwrap().is_empty() {
                    build_grid(self, &ctx, ui);
                }
            });
        });
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let media_files = filter_media_files(process_entries(filter_valid_paths(args)));

    ffmpeg::init().unwrap();

    let native_options = eframe::NativeOptions {
        viewport: egui::viewport::ViewportBuilder {
            min_inner_size: Some(egui::vec2(400.0, 400.0)),
            inner_size: Some(egui::vec2(600.0, 400.0)),
            position: Some(egui::pos2(100.0, 100.0)),
            ..Default::default()
        },
        ..Default::default()
    };

    match eframe::run_native(
        "Gallery",
        native_options,
        Box::new(|cc| Box::new(App::new(cc, media_files))),
    ) {
        Ok(_) => {}
        Err(err) => {
            println!("Error: {}", err);
        }
    }
}
