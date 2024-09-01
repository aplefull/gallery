// This file is for the future addition of imagemagick as loader.
// Currently it's an absolute pain to use it on windows, so it's not used.

pub fn load_thumbnail_with_magick(
    ctx: &egui::Context,
    file: &PathBuf,
    size: f32,
) -> Option<egui::TextureHandle> {
    START.call_once(|| {
        magick_wand_genesis();
    });

    let wand = MagickWand::new();
    match wand.read_image(file.to_str().unwrap()) {
        Ok(_) => {}
        Err(err) => {
            println!("Error reading image with magick: {:?}", err);

            return None;
        }
    }

    wand.thumbnail_image(size as usize, size as usize);

    let frame = match wand.export_image_pixels(0, 0, size as usize, size as usize, "RGBA") {
        Some(img) => img,
        None => {
            println!("Error exporting image pixels");

            return None;
        }
    };

    let color_image = ColorImage::from_rgba_unmultiplied([size as usize, size as usize], &frame);
    let name = file
        .file_name()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_string();

    Some(ctx.load_texture(name, color_image, Default::default()))
}

pub fn load_image_with_magick(
    ctx: &egui::Context,
    file: &PathBuf,
) -> Result<(Vec<egui::TextureHandle>, Vec<Delay>), Box<dyn std::error::Error>> {
    START.call_once(|| {
        magick_wand_genesis();
    });

    let wand = MagickWand::new();
    wand.read_image(file.to_str().unwrap())?;

    let image_width = wand.get_image_width();
    let image_height = wand.get_image_height();

    let frame = match wand.export_image_pixels(0, 0, image_width, image_height, "RGBA") {
        Some(img) => img,
        None => {
            println!("Error exporting image pixels");

            return Err("Error exporting image pixels".into());
        }
    };

    println!("{:?}", frame.len());

    let color_image =
        ColorImage::from_rgba_unmultiplied([image_width as usize, image_height as usize], &frame);

    let mut textures = Vec::new();
    let mut delays = Vec::new();

    let color_images = vec![color_image];

    for (i, color_image) in color_images.iter().enumerate() {
        let name = file
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_owned()
            + &format!(" - Frame {}", i + 1);
        let texture = ctx.load_texture(name, color_image.to_owned(), Default::default());
        textures.push(texture);
    }

    Ok((textures, delays))
}


