use std::path::PathBuf;

use eframe::egui::{FontId, Pos2, Stroke};

use crate::egui::epaint::TextureHandle;
use crate::egui::{self, Response, Sense, Ui, Widget};
use crate::utils::calculate_contain_size;

pub struct ImageFrame {
    texture: TextureHandle,
    width: f32,
    height: f32,
    path: PathBuf,
    draw_border: bool,
}

impl ImageFrame {
    pub fn new(texture: &TextureHandle, width: f32, height: f32, path: &PathBuf, draw_border: bool) -> Self {
        Self {
            texture: texture.clone(),
            width,
            height,
            draw_border,
            path: path.clone(),
        }
    }
}

impl Widget for ImageFrame {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired_size = egui::vec2(self.width, self.height);

        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        if ui.is_rect_visible(rect) {
            let texture_size = calculate_contain_size(
                self.width,
                self.height,
                self.texture.size()[0] as f32,
                self.texture.size()[1] as f32,
            );
            
            let sized_texture =
                egui::load::SizedTexture::new(self.texture.id(), texture_size);

            let image = egui::Image::new(sized_texture).sense(egui::Sense::click());

            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(200));

            let response = ui.put(rect, image);

            let extension = self.path.extension().unwrap_or_default().to_str().unwrap_or_default();
            
            ui.painter().text(
                Pos2::from([rect.left() + 5.0, rect.bottom() - 5.0]),
                egui::Align2::LEFT_BOTTOM,
                &extension,
                FontId::monospace(14.0),
                egui::Color32::LIGHT_RED
            );

            if self.draw_border {
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(3.0, egui::Color32::from_rgb(180, 123, 182)),
                );
            }

            response
        } else {
            response
        }
    }
}
