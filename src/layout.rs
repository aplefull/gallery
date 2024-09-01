use crate::utils::{
    calculate_contain_size, calculate_thumbnail_layout, get_window_size,
};
use crate::video_entry::VideoEntry;
use crate::image_entry::ImageEntry;
use crate::widgets::image_frame::ImageFrame;
use crate::widgets::video_player::VideoPlayer;
use crate::{App, CurrentEntry, MediaType};
use eframe::egui::{self, FontId};
use std::path::PathBuf;
use trash;

pub fn build_grid(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
        let mut entries = app.entries.lock().unwrap();
        let to_delete = entries
            .iter()
            .filter(|entry| entry.marked)
            .map(|entry| entry.path.clone())
            .collect::<Vec<PathBuf>>();

        for file in to_delete {
            match trash::delete(&file) {
                Ok(_) => {
                    entries.retain(|entry| entry.path != file);
                }
                Err(err) => {
                    println!("Error deleting file: {:?}", err);
                }
            }
        }
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        let available_width = ui.available_width();
        let gap = 10.0;
        ui.set_width(available_width);

        let min_thumbnail_width = app.settings.min_thumbnail_size;
        let max_columns_count = app.settings.max_columns_count;
        let (columns, thumbnail_width) = calculate_thumbnail_layout(
            available_width,
            min_thumbnail_width as f32,
            gap,
            max_columns_count,
        );

        app.entries
            .lock()
            .unwrap()
            .sort_by(|a, b| a.path.cmp(&b.path));

        let entries_paths = app
            .entries
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<PathBuf>>();

        let mut entries_to_toggle = vec![];

        for chunk in app.entries.lock().unwrap().chunks_mut(columns) {
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    for entry in chunk.iter_mut() {
                        let i_f = ImageFrame::new(
                            &entry.thumbnail,
                            thumbnail_width,
                            thumbnail_width,
                            &entry.path,
                            entry.marked,
                        );
                        let image_res = ui.add(i_f);

                        if image_res.clicked() {
                            let is_shift_down = ctx.input(|i| i.modifiers.shift);
                            let current_entry_index = entries_paths
                                .iter()
                                .position(|path| path == &entry.path)
                                .unwrap();
                            let last_marked_entry_index = match app.last_marked_entry_index {
                                Some(index) => index,
                                None => current_entry_index,
                            };

                            if !is_shift_down {
                                let marked = entry.marked;
                                entry.marked = !marked;
                            } else {
                                let start =
                                    std::cmp::min(last_marked_entry_index, current_entry_index);
                                let end =
                                    std::cmp::max(last_marked_entry_index, current_entry_index);

                                for i in start..=end {
                                    entries_to_toggle.push(i);
                                }
                            }
                            app.last_marked_entry_index = Some(current_entry_index);
                        }

                        if image_res.secondary_clicked() {
                            if entry.media_type == MediaType::Video {
                                let video = VideoEntry::new(&entry.path);

                                match video {
                                    Some(video) => {
                                        app.current_entry = Some(CurrentEntry {
                                            media_type: MediaType::Video,
                                            image: None,
                                            video: Some(video),
                                        });
                                    }
                                    None => {
                                        println!("Failed to load video: {:?}", entry.path);

                                        return;
                                    }
                                }

                                return;
                            }

                            let image = ImageEntry::new(&entry.path, ctx);

                            app.current_entry = Some(CurrentEntry {
                                media_type: match &image {
                                    Some(image) => image.media_type.clone(),
                                    None => MediaType::ImageAnimated,
                                },
                                image,
                                video: None,
                            });
                        }
                    }
                });
            });

            ui.add_space(gap);
        }

        for i in entries_to_toggle {
            app.entries.lock().unwrap()[i].marked = true;
        }
    });
}

pub fn build_preview(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui) {
    let window_size = get_window_size(ctx);

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.current_entry = None;
    }

    if ctx.input(|i| i.pointer.secondary_pressed()) {
        app.current_entry = None;
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
        match &mut app.current_entry {
            Some(entry) => {
                match &mut entry.video {
                    Some(video) => {
                        video.toggle_playback();
                        return ();
                    }
                    None => {
                        return ();
                    }
                };
            }
            None => {
                println!("No current entry found");
                return ();
            }
        };
    }

    if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowLeft)) {
        let current_entry = match &mut app.current_entry {
            Some(entry) => entry,
            None => {
                println!("No current entry found");
                return ();
            }
        };

        match current_entry.video {
            Some(ref mut video) => {
                let is_shift_down = ctx.input(|i| i.modifiers.shift);

                if !is_shift_down {
                    return ();
                }

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    video.seek_relative(5 * 1000);
                } else {
                    video.seek_relative(-5 * 1000);
                }

                return ();
            }
            None => {}
        }

        let current_entry_path = match &current_entry.media_type {
            MediaType::ImageStill | MediaType::ImageAnimated => match &current_entry.image {
                Some(image) => image.path.clone(),
                None => {
                    println!("No image found for current entry");
                    return ();
                }
            },
            MediaType::Video => match &current_entry.video {
                Some(video) => video.path.clone(),
                None => {
                    println!("No video found for current entry");
                    return ();
                }
            },
        };

        let current_entry_index = app
            .entries
            .lock()
            .unwrap()
            .iter()
            .position(|entry| entry.path == current_entry_path)
            .unwrap();

        let previous_entry_index = if current_entry_index == 0 {
            app.entries.lock().unwrap().len() - 1
        } else {
            current_entry_index - 1
        };

        let next_entry_index = if current_entry_index + 1 >= app.entries.lock().unwrap().len() {
            0
        } else {
            current_entry_index + 1
        };

        let index_to_use = if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            next_entry_index
        } else {
            previous_entry_index
        };

        let next_entry = &app.entries.lock().unwrap()[index_to_use];

        if next_entry.media_type == MediaType::Video {
            let video = VideoEntry::new(&next_entry.path);

            match video {
                Some(video) => {
                    app.current_entry = Some(CurrentEntry {
                        media_type: MediaType::Video,
                        image: None,
                        video: Some(video),
                    });
                }
                None => {
                    println!("Failed to load video: {:?}", next_entry.path);
                }
            }

            return;
        }

        let image = ImageEntry::new(&next_entry.path, ctx);

        app.current_entry = Some(CurrentEntry {
            media_type: match &image {
                Some(image) => image.media_type.clone(),
                None => MediaType::ImageAnimated,
            },
            image,
            video: None,
        });
    }

    let entry = match &mut app.current_entry {
        Some(entry) => entry,
        None => {
            println!("No current entry found");
            return ();
        }
    };

    if entry.media_type == MediaType::Video {
        let video = match &mut entry.video {
            Some(video) => video,
            None => {
                println!("No video found for current entry");
                return ();
            }
        };

        let video_player = VideoPlayer::new(video, ctx);

        ui.centered_and_justified(|ui| {
            ui.add(video_player);
        });

        return;
    }

    let texture = match &entry.media_type {
        MediaType::ImageStill | MediaType::ImageAnimated => match entry.image.as_mut() {
            Some(image) => image.get_current_frame(ctx),
            None => {
                println!("No image found for current entry");
                return ();
            }
        },
        MediaType::Video => match entry.video.as_mut() {
            Some(video) => video.get_current_frame(ctx),
            None => {
                println!("No video found for current entry");
                return ();
            }
        },
    };

    if texture.is_none() {
        println!("No texture found for current entry");
        return;
    }

    let texture = texture.unwrap();

    let texture_size = calculate_contain_size(
        window_size.x - 10.0,
        window_size.y - 10.0,
        texture.size()[0] as f32,
        texture.size()[1] as f32,
    );

    let sized_texture =
        egui::load::SizedTexture::new(texture.id(), [texture_size.0, texture_size.1]);

    let img = egui::Image::new(sized_texture);
    ui.centered_and_justified(|ui| {
        ui.add(img);
    });

    let path = match &entry.media_type {
        MediaType::ImageStill | MediaType::ImageAnimated => match &entry.image {
            Some(image) => &image.path,
            None => {
                println!("No image found for current entry");
                return ();
            }
        },
        MediaType::Video => match &entry.video {
            Some(video) => &video.path,
            None => {
                println!("No video found for current entry");
                return ();
            }
        },
    };

    let extension = path
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default();
    let number_of_frames = match &entry.media_type {
        MediaType::ImageStill | MediaType::ImageAnimated => match &entry.image {
            Some(image) => image.get_number_of_frames(),
            None => 0,
        },
        MediaType::Video => 0,
    };

    let resolution = format!("{}x{}", texture.size()[0], texture.size()[1]);

    ui.painter().text(
        egui::Pos2::from([5.0, 5.0]),
        egui::Align2::LEFT_TOP,
        &extension,
        FontId::monospace(14.0),
        egui::Color32::WHITE,
    );

    ui.painter().text(
        egui::Pos2::from([5.0, 20.0]),
        egui::Align2::LEFT_TOP,
        &format!("{} frames", number_of_frames),
        FontId::monospace(14.0),
        egui::Color32::WHITE,
    );

    ui.painter().text(
        egui::Pos2::from([5.0, 35.0]),
        egui::Align2::LEFT_TOP,
        &resolution,
        FontId::monospace(14.0),
        egui::Color32::WHITE,
    );
}
