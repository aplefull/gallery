use std::path::PathBuf;

use eframe::egui::{ColorImage, FontId, Pos2, Stroke};

use crate::egui::epaint::TextureHandle;
use crate::egui::{self, Response, Sense, Ui, Widget};
use crate::utils::{calculate_contain_size, format_time};
use crate::video_entry::VideoEntry;

pub struct Icon {
    path: PathBuf,
    size: f32,
}

impl Icon {
    pub fn new(path: PathBuf, size: f32) -> Self {
        Self { path, size }
    }
}

impl Widget for Icon {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = self.size;
        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            painter.rect(
                rect,
                0.0,
                egui::Color32::from_black_alpha(100),
                Stroke::new(1.0, egui::Color32::WHITE),
            );

            /* let reader = image::ImageReader::open(self.path)
                .unwrap()
                .with_guessed_format()
                .unwrap();
            let image = reader.decode().unwrap().into_rgba8().into_flat_samples();
            let image = ColorImage::from_rgba_unmultiplied(
                [size as usize, size as usize],
                image.as_slice(),
            );

            let texture = ui
                .ctx()
                .load_texture("icon", image.to_owned(), Default::default());

            let response = ui.put(rect, egui::Image::new(&texture).sense(Sense::hover())); */

            response
        } else {
            response
        }
    }
}

pub struct VideoVolumeWidget {
    volume: f32,
    muted: bool,
}

impl VideoVolumeWidget {
    pub fn new() -> Self {
        Self {
            volume: 100.0,
            muted: false,
        }
    }
}

impl Widget for VideoVolumeWidget {
    fn ui(self, ui: &mut Ui) -> Response {
        let volume = self.volume;
        let muted = self.muted;

        let avail_width = ui.available_width();
        let avail_height = ui.available_height();

        let (id, rect) = ui.allocate_space(egui::vec2(avail_width, avail_height));

        /* ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(100, 100, 100, 255),
                );
        */
        let res = ui.interact(rect, ui.id(), Sense::click_and_drag());

        res
    }
}

pub struct VideoPlayer {
    texture: Option<TextureHandle>,
    current_time: u64,
    duration: u64,
}

impl VideoPlayer {
    pub fn new(video: &mut VideoEntry, ctx: &egui::Context) -> Self {
        let texture_handle = &video.get_current_frame(ctx);

        Self {
            texture: texture_handle.clone(),
            current_time: video.current_time,
            duration: video.video_duration,
        }
    }
}

impl Widget for VideoPlayer {
    fn ui(self, ui: &mut Ui) -> Response {
        let bottom_bar_height = 30.0;
        let progress_bar_height = bottom_bar_height / 2.0;
        let gap = 10.0;

        let screen_rect = ui.input(|i: &egui::InputState| i.screen_rect());
        let video_surface_rect = egui::Rect::from_min_max(
            Pos2::new(screen_rect.left(), screen_rect.top()),
            Pos2::new(
                screen_rect.right(),
                screen_rect.bottom() - bottom_bar_height,
            ),
        );

        let response = ui.allocate_rect(video_surface_rect, Sense::hover());

        let texture = match &self.texture {
            Some(texture) => texture,
            None => {
                println!("No texture found for video player");

                return response;
            }
        };

        if ui.is_rect_visible(video_surface_rect) {
            ui.painter()
                .rect_filled(video_surface_rect, 0.0, egui::Color32::RED);

            let texture_size = calculate_contain_size(
                video_surface_rect.width(),
                video_surface_rect.height(),
                texture.size()[0] as f32,
                texture.size()[1] as f32,
            );

            let sized_texture = egui::load::SizedTexture::new(texture.id(), texture_size);

            let video = egui::Image::new(sized_texture).sense(egui::Sense::click());
            let response = ui.put(video_surface_rect, video);

            let bottom_bar_rect = egui::Rect::from_min_max(
                Pos2::new(screen_rect.left(), screen_rect.bottom() - bottom_bar_height),
                Pos2::new(screen_rect.right(), screen_rect.bottom()),
            );

            ui.painter().rect_filled(
                bottom_bar_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(155, 155, 155, 255),
            );

            let play_icon_rect = egui::Rect::from_min_max(
                Pos2::new(
                    bottom_bar_rect.left(),
                    bottom_bar_rect.bottom() - bottom_bar_height,
                ),
                Pos2::new(
                    bottom_bar_rect.left() + bottom_bar_height,
                    bottom_bar_rect.bottom(),
                ),
            );

            let play_icon = Icon::new(
                PathBuf::from("./assets/icons/play.png"),
                play_icon_rect.width(),
            );

            let play_button_res = ui.put(play_icon_rect, play_icon);

            let time_text = format!(
                "{} / {}",
                format_time(self.current_time),
                format_time(self.duration)
            );

            let text_rect = ui
                .painter()
                .layout(
                    time_text.to_string(),
                    FontId::monospace(12.0),
                    egui::Color32::WHITE,
                    10000.0,
                )
                .rect;

            let text_pos = Pos2::new(
                play_icon_rect.right() + gap,
                bottom_bar_rect.bottom() - text_rect.height(),
            );

            let text_rect = ui.painter().text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                time_text,
                FontId::monospace(14.0),
                egui::Color32::WHITE,
            );

            let progress_bar_background_rect = egui::Rect::from_min_max(
                Pos2::new(
                    text_rect.right() + gap,
                    (bottom_bar_height - progress_bar_height) / 2.0 + bottom_bar_rect.top(),
                ),
                Pos2::new(
                    bottom_bar_rect.right() - gap,
                    bottom_bar_rect.bottom() - (bottom_bar_height - progress_bar_height) / 2.0,
                ),
            );

            ui.painter().rect_filled(
                progress_bar_background_rect,
                10.0,
                egui::Color32::from_rgba_unmultiplied(100, 100, 100, 255),
            );

            let progress_bar_rect = egui::Rect::from_min_max(
                Pos2::new(
                    progress_bar_background_rect.left(),
                    progress_bar_background_rect.top(),
                ),
                Pos2::new(
                    progress_bar_background_rect.left()
                        + (progress_bar_background_rect.width() as f32
                            * (self.current_time as f32 / self.duration as f32)),
                    progress_bar_background_rect.bottom(),
                ),
            );

            ui.painter().rect_filled(
                progress_bar_rect,
                10.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 255),
            );

            let full_screen_icon_rect = egui::Rect::from_min_max(
                Pos2::new(
                    bottom_bar_rect.right() - bottom_bar_height,
                    bottom_bar_rect.top(),
                ),
                Pos2::new(bottom_bar_rect.right(), bottom_bar_rect.bottom()),
            );

            let full_screen_icon = Icon::new(
                PathBuf::from("./assets/icons/full_screen.png"),
                full_screen_icon_rect.width(),
            );

            let full_screen_res = ui.put(full_screen_icon_rect, full_screen_icon);

            response
        } else {
            response
        }
    }
}
