use crate::{
    measure_time,
    utils::{calculate_contain_size, is_image, load_texture, SharedTextureManager},
    MediaType,
};
use dicom::pixeldata::PixelDecoder;
use eframe::egui::{ColorImage, Context as EguiContext, TextureHandle};
use ffmpeg_next::{
    codec::context::Context as CodecContext,
    format::{self, pixel::Pixel},
    media::Type::Video as VideoType,
    software::scaling::{context::Context as ScalingContext, flag::Flags},
    util::frame::video::Video as VideoFrame,
};
use image::{codecs, AnimationDecoder, Delay, FlatSamples, ImageBuffer, Rgb};
use imagepipe::{ImageSource, Pipeline};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

pub struct ImageFrame {
    pub texture: TextureHandle,
    pub delay: Delay,
}

impl ImageFrame {
    pub fn from_raw_frame(
        frame: RawImageFrame,
        size: [usize; 2],
        texture_manager: &SharedTextureManager,
    ) -> Self {
        let color_image = ColorImage::from_rgba_unmultiplied(size, frame.pixels.as_slice());

        let texture = load_texture(texture_manager.clone(), color_image);

        ImageFrame {
            texture,
            delay: frame.delay,
        }
    }
}
pub struct RawImageFrame {
    pub pixels: FlatSamples<Vec<u8>>,
    pub delay: Delay,
}

impl RawImageFrame {
    pub fn from_frame(frame: image::Frame) -> Self {
        let delay = frame.delay();
        let pixels = frame.into_buffer().into_flat_samples();

        RawImageFrame { pixels, delay }
    }

    pub fn from_image(image: image::DynamicImage) -> Self {
        let pixels = image.into_rgba8().into_flat_samples();
        let delay = Delay::from_numer_denom_ms(8333, 100);
        RawImageFrame { pixels, delay }
    }
}

pub struct StillImage {
    pub texture: TextureHandle,
}

impl StillImage {
    pub fn from_raw_frame(
        frame: RawImageFrame,
        size: [usize; 2],
        texture_manager: &SharedTextureManager,
    ) -> Self {
        let color_image = ColorImage::from_rgba_unmultiplied(size, frame.pixels.as_slice());
        let texture = load_texture(texture_manager.clone(), color_image);

        StillImage { texture }
    }

    pub fn from_pixels(
        pixels: Vec<u8>,
        size: [usize; 2],
        texture_manager: &SharedTextureManager,
    ) -> Self {
        let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        let texture = load_texture(texture_manager.clone(), color_image);

        StillImage { texture }
    }
}

pub struct AnimatedImage {
    pub frames: Vec<ImageFrame>,
}

impl AnimatedImage {
    pub fn from_raw_frames(
        frames: Vec<RawImageFrame>,
        size: [usize; 2],
        texture_manager: &SharedTextureManager,
    ) -> Self {
        let frames = frames
            .into_iter()
            .map(|frame| ImageFrame::from_raw_frame(frame, size, texture_manager))
            .collect();

        AnimatedImage { frames }
    }
}

pub enum Image {
    Still(StillImage),
    Animated(AnimatedImage),
}

impl Image {
    pub fn get_texture(&self) -> Option<TextureHandle> {
        match self {
            Image::Still(still_image) => Some(still_image.texture.clone()),
            Image::Animated(animated_image) => {
                let frame = match animated_image.frames.first() {
                    Some(frame) => frame,
                    None => return None,
                };

                Some(frame.texture.clone())
            }
        }
    }
}

#[derive(Debug)]
pub enum ImageFormat {
    Dicom,
    Rpgmv,
    JpegLs,
    JBig1,
    JBig2,
    Unknown,
}

pub struct ImageEntry {
    pub is_animated: bool,
    pub media_type: MediaType,
    pub path: PathBuf,
    image: Image,
    last_frame_time: std::time::Instant,
    current_frame_index: usize,
}

impl ImageEntry {
    pub fn new(image_path: &PathBuf, ctx: &EguiContext) -> Option<Self> {
        let image = match ImageEntry::load_image(ctx, image_path) {
            Ok(image) => image,
            Err(err) => {
                println!("Error loading image: {:?}", err);

                return None;
            }
        };

        Some(ImageEntry {
            is_animated: matches!(image, Image::Animated(_)),
            media_type: if matches!(image, Image::Animated(_)) {
                MediaType::ImageAnimated
            } else {
                MediaType::ImageStill
            },
            path: image_path.clone(),
            last_frame_time: std::time::Instant::now(),
            current_frame_index: 0,
            image,
        })
    }

    pub fn try_guess_format(
        file_path: &PathBuf,
    ) -> Result<ImageFormat, Box<dyn std::error::Error>> {
        let mut file = std::fs::File::open(file_path)?;

        let mut buffer = [0; 256];
        file.read_exact(&mut buffer)?;

        // DICOM
        if buffer.len() >= 132 && &buffer[128..132] == b"DICM" {
            return Ok(ImageFormat::Dicom);
        }

        // RPGMV
        let rpgmv_bytes = [0x52, 0x50, 0x47, 0x4D, 0x56];
        if buffer.len() >= 5 && &buffer[0..5] == rpgmv_bytes {
            return Ok(ImageFormat::Rpgmv);
        }

        // JPEG-LS
        let jpeg_ls_bytes = [0xFF, 0xD8, 0xFF, 0xF7];
        if buffer.len() >= 4 && &buffer[0..4] == jpeg_ls_bytes {
            return Ok(ImageFormat::JpegLs);
        }

        Ok(ImageFormat::JBig2)
    }

    pub fn default_texture(texture_manager: SharedTextureManager) -> TextureHandle {
        let image_bytes = include_bytes!("assets/images/missing.png");
        let image_buffer = image::load_from_memory(image_bytes).unwrap();

        let color_image = ColorImage::from_rgb(
            [256, 256],
            image_buffer.into_rgb8().into_flat_samples().as_slice(),
        );

        load_texture(texture_manager, color_image)
    }

    pub fn get_current_frame(&mut self, ctx: &EguiContext) -> Option<TextureHandle> {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_frame_time).as_secs_f64();

        match &self.image {
            Image::Still(still_image) => {
                if elapsed >= 1.0 {
                    self.last_frame_time = now;
                }

                ctx.request_repaint();

                return Some(still_image.texture.clone());
            }

            Image::Animated(animated_image) => {
                let delay = match animated_image.frames.get(self.current_frame_index) {
                    Some(frame) => {
                        let (num, den) = frame.delay.numer_denom_ms();

                        let res = num as f64 / den as f64;

                        match res {
                            0.0 => 83.33,
                            _ => res,
                        }
                    }
                    None => 83.33,
                };

                if elapsed >= delay / 1000.0 {
                    self.last_frame_time = now;

                    if self.is_animated {
                        self.current_frame_index =
                            (self.current_frame_index + 1) % animated_image.frames.len();
                    }
                }

                ctx.request_repaint();

                return Some(
                    animated_image
                        .frames
                        .get(self.current_frame_index)?
                        .texture
                        .clone(),
                );
            }
        }
    }

    pub fn get_number_of_frames(&self) -> usize {
        match &self.image {
            Image::Still(_) => 1,
            Image::Animated(animated_image) => animated_image.frames.len(),
        }
    }

    pub fn load_image(
        ctx: &EguiContext,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let texture_manager = ctx.tex_manager();

        match ImageEntry::load_image_native(ctx, file) {
            Ok(image) => return Ok(image),
            Err(error) => {
                println!("Failed to load image using native rust loader, trying other options... Error: {:?}", error);
            }
        }

        let format = ImageEntry::try_guess_format(file)?;

        match format {
            ImageFormat::Dicom => ImageEntry::load_dicom_image(&texture_manager, file),
            ImageFormat::Rpgmv => ImageEntry::load_rpgmv_image(&texture_manager, file),
            ImageFormat::JpegLs => ImageEntry::load_jpeg_ls_image(&texture_manager, file),
            ImageFormat::JBig1 => ImageEntry::load_jbig_image(&texture_manager, file),
            ImageFormat::JBig2 => ImageEntry::load_jbig_image(&texture_manager, file),
            ImageFormat::Unknown => match ImageEntry::load_raw_image(&texture_manager, file) {
                Ok(image) => Ok(image),
                Err(error) => {
                    println!(
                        "Failed to load image using rawloader, trying ffmpeg... Error: {:?}",
                        error
                    );

                    ImageEntry::load_image_ffmpeg(&texture_manager, file, None, false)
                }
            },
        }
    }

    pub fn load_image_native(
        ctx: &EguiContext,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let reader = image::ImageReader::open(file)?.with_guessed_format()?;
        let image_format = reader.format();
        let texture_manager = ctx.tex_manager();
        let mut frames = Vec::new();

        match image_format {
            Some(image::ImageFormat::Gif) => {
                let input_stream = std::fs::File::open(file)?;
                let gif_decoder = codecs::gif::GifDecoder::new(BufReader::new(input_stream))?;

                let decoded_frames = gif_decoder.into_frames().collect_frames()?;
                frames = decoded_frames
                    .iter()
                    .map(|frame| RawImageFrame::from_frame(frame.to_owned()))
                    .collect();
            }

            Some(image::ImageFormat::WebP) => {
                let input_stream = std::fs::File::open(file)?;
                let webp_decoder = codecs::webp::WebPDecoder::new(BufReader::new(input_stream))?;

                let decoded_frames = webp_decoder.into_frames().collect_frames()?;
                frames = decoded_frames
                    .iter()
                    .map(|frame| RawImageFrame::from_frame(frame.to_owned()))
                    .collect();
            }

            Some(image::ImageFormat::Png) => {
                let input_stream = std::fs::File::open(file)?;
                let apng_decoder = codecs::png::PngDecoder::new(BufReader::new(input_stream))?;

                let is_apng = apng_decoder.is_apng().unwrap_or(false);

                if is_apng {
                    let decoded_frames = apng_decoder.apng()?.into_frames().collect_frames()?;
                    frames = decoded_frames
                        .iter()
                        .map(|frame| RawImageFrame::from_frame(frame.to_owned()))
                        .collect();
                }
            }

            _ => {}
        };

        let image = match reader.decode() {
            Ok(image) => image,
            Err(error) => {
                return Err(Box::new(error));
            }
        };

        let image_size = [image.width() as usize, image.height() as usize];

        if frames.is_empty() {
            frames.push(RawImageFrame::from_image(image));
        }

        if frames.len() == 1 {
            let still_image =
                StillImage::from_raw_frame(frames.pop().unwrap(), image_size, &texture_manager);

            return Ok(Image::Still(still_image));
        }

        let animated_image = AnimatedImage::from_raw_frames(frames, image_size, &texture_manager);

        Ok(Image::Animated(animated_image))
    }

    // TODO ffmpeg crashes and burns without any way to recover on some unsupported files
    // Ideally, it should run in a separate process. But IPC is painfull and
    // opening a lot of images will spawn a lot of processes, so this needs to be controlled
    pub fn load_image_ffmpeg(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
        size: Option<f32>,
        is_thumbnail: bool,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let mut ictx = format::input(file)?;
        let input = ictx
            .streams()
            .best(VideoType)
            .ok_or("No video stream found")?;

        let video_stream_index = input.index();
        let context = CodecContext::from_parameters(input.parameters())?;
        let mut decoder = context.decoder().video()?;

        let destination_size = if is_thumbnail {
            let size = size.unwrap_or(256.0);
            let (w, h) =
                calculate_contain_size(size, size, decoder.width() as f32, decoder.height() as f32);

            (w.trunc() as u32, h.trunc() as u32)
        } else {
            (decoder.width(), decoder.height())
        };

        let mut scaler = ScalingContext::get(
            decoder.format(),
            decoder.width(),
            decoder.height(),
            Pixel::RGBA,
            destination_size.0,
            destination_size.1,
            Flags::BILINEAR,
        )?;

        let mut buffers = Vec::new();
        let mut pts_values = Vec::new();

        let mut image_width = 0;
        let mut image_height = 0;

        let time_base = input.time_base();

        for (stream, packet) in ictx.packets() {
            if stream.index() == video_stream_index {
                decoder.send_packet(&packet)?;
                image_height = decoder.height();
                image_width = decoder.width();

                let mut frame = VideoFrame::empty();
                while decoder.receive_frame(&mut frame).is_ok() {
                    let mut rgba_frame = VideoFrame::empty();
                    scaler.run(&frame, &mut rgba_frame)?;

                    let width = rgba_frame.width() as usize;
                    let height = rgba_frame.height() as usize;
                    let stride = rgba_frame.stride(0);
                    let expected_size = width * height * 4;

                    let mut buffer = Vec::with_capacity(expected_size);

                    for y in 0..height {
                        let start = y * stride;
                        let end = start + width * 4;
                        buffer.extend_from_slice(&rgba_frame.data(0)[start..end]);
                    }

                    buffers.push(buffer);
                    pts_values.push(frame.pts().unwrap_or(0));

                    if is_thumbnail {
                        image_width = width as u32;
                        image_height = height as u32;

                        break;
                    }
                }
            }
        }

        if buffers.len() == 1 {
            let buffer = buffers.pop().unwrap();
            let color_image = ColorImage::from_rgba_unmultiplied(
                [image_width as usize, image_height as usize],
                &buffer,
            );

            return Ok(Image::Still(StillImage {
                texture: load_texture(texture_manager.clone(), color_image),
            }));
        }

        let mut delays = Vec::new();
        for i in 0..pts_values.len() - 1 {
            let delay = (pts_values[i + 1] - pts_values[i]) as f64 * f64::from(time_base);
            delays.push(Delay::from_numer_denom_ms((delay * 1000.0) as u32, 1));
        }

        delays.push(
            delays
                .last()
                .unwrap_or(&Delay::from_numer_denom_ms(83, 1))
                .clone(),
        );

        let frames = buffers
            .into_iter()
            .map(|buffer| {
                let color_image = ColorImage::from_rgba_unmultiplied(
                    [image_width as usize, image_height as usize],
                    buffer.as_slice(),
                );

                let texture = load_texture(texture_manager.clone(), color_image);

                ImageFrame {
                    texture,
                    delay: delays.pop().unwrap(),
                }
            })
            .collect();

        let animated_image = AnimatedImage { frames };

        Ok(Image::Animated(animated_image))
    }

    // TODO Split everything thumbnail related to a separate ThumbnailLoader in order to clean up a bit
    pub fn load_thumbnail(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
        size: f32,
    ) -> Option<Image> {
        let is_image = is_image(&file);

        if !is_image {
            match ImageEntry::load_image_ffmpeg(texture_manager, &file, Some(size), true) {
                Ok(image) => return Some(image),
                Err(err) => {
                    println!("Failed to load thumbnail using ffmpeg: {:?}", err);
                }
            };

            return None;
        }

        match ImageEntry::load_thumbnail_native(texture_manager, &file, size) {
            Ok(texture) => return Some(texture),
            Err(err) => {
                println!(
                    "Failed to load thumbnail using native loader, trying other options... Error: {:?}",
                    err
                );
            }
        }

        // TODO maybe this can be done better, so it's not duplicated
        let img_format = ImageEntry::try_guess_format(file).unwrap_or(ImageFormat::Unknown);

        let maybe_image = match img_format {
            ImageFormat::Dicom => ImageEntry::load_dicom_image(texture_manager, file),
            ImageFormat::Rpgmv => ImageEntry::load_rpgmv_image(texture_manager, file),
            ImageFormat::JpegLs => ImageEntry::load_jpeg_ls_image(texture_manager, file),
            ImageFormat::JBig1 => ImageEntry::load_jbig_image(texture_manager, file),
            ImageFormat::JBig2 => ImageEntry::load_jbig_image(texture_manager, file),
            ImageFormat::Unknown => ImageEntry::load_raw_image(texture_manager, file),
        };

        // TODO this returns full image instead of a thumbnail
        match maybe_image {
            Ok(image) => return Some(image),
            Err(err) => {
                println!("Failed to load thumbnail using other loaders: {:?}", err);

                // TODO this returns before ffmpeg attempts to decode an image, since ffmpeg is unstable for now
                return None;
            }
        }

        match ImageEntry::load_image_ffmpeg(texture_manager, &file, Some(size), true) {
            Ok(texture) => return Some(texture),
            Err(err) => {
                println!("Failed to load thumbnail using ffmpeg: {:?}", err);
            }
        }

        None
    }

    fn load_thumbnail_native(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
        size: f32,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let image_reader = image::ImageReader::open(file)?;
        let image = image_reader.decode()?;

        let thumbnail = image.thumbnail(size as u32, size as u32);
        let thumbnail_size = [thumbnail.width() as usize, thumbnail.height() as usize];

        let thumbnail_bytes = thumbnail.into_rgba8();
        let flat_samples = thumbnail_bytes.into_flat_samples();

        let color_image =
            ColorImage::from_rgba_unmultiplied(thumbnail_size, flat_samples.as_slice());

        let texture = load_texture(texture_manager.clone(), color_image);

        Ok(Image::Still(StillImage { texture }))
    }

    fn load_rpgmv_image(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        // All rpgmv images are just png files with a custom header
        // Simply replacing the header with a valid png header produces a valid png file
        let png_header = [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82];
        let header_length = png_header.len();

        let mut file = File::open(file)?;
        let mut buffer = Vec::new();

        file.read_to_end(&mut buffer)?;

        buffer.splice(0..header_length * 2, png_header.iter().cloned());

        let image = image::load_from_memory(&buffer)?;

        let image_width = image.width() as usize;
        let image_height = image.height() as usize;

        return Ok(Image::Still(StillImage::from_pixels(
            image.into_rgba8().into_flat_samples().as_slice().to_vec(),
            [image_width, image_height],
            texture_manager,
        )));
    }

    fn load_dicom_image(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let file = dicom::object::open_file(file)?;
        let pixel_data = file.decode_pixel_data()?;
        let frames_count = pixel_data.number_of_frames();

        let mut frames = Vec::new();
        for i in 0..frames_count {
            let img = pixel_data.to_dynamic_image(i)?;
            frames.push(img);
        }

        let width = pixel_data.columns();
        let height = pixel_data.rows();

        let image_width = width as usize;
        let image_height = height as usize;

        if frames.len() == 1 {
            let raw_frame = RawImageFrame::from_image(frames.pop().unwrap());
            let still_image =
                StillImage::from_raw_frame(raw_frame, [image_width, image_height], texture_manager);

            return Ok(Image::Still(still_image));
        }

        let raw_frames = frames
            .into_iter()
            .map(|frame| RawImageFrame::from_image(frame))
            .collect();

        let animated_image = AnimatedImage::from_raw_frames(
            raw_frames,
            [image_width, image_height],
            texture_manager,
        );

        Ok(Image::Animated(animated_image))
    }

    fn load_raw_image(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        // TODO uncool
        // Imagepipe crate is pretty slow and adds a lot to the executable size.
        // It would be great to implement in-house demosaic and remove it from deps
        let raw_image = rawloader::decode_file(file)?;
        let image_source = ImageSource::Raw(raw_image);

        let mut pipeline = Pipeline::new_from_source(image_source)?;

        pipeline.run(None);

        let image = pipeline.output_8bit(None)?;

        let image = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
            image.width as u32,
            image.height as u32,
            image.data,
        )
        .ok_or_else(|| "Failed to create image buffer")?;

        let dynamic_image = image::DynamicImage::from(image);

        let image_width = dynamic_image.width() as usize;
        let image_height = dynamic_image.height() as usize;

        // TODO add StillImage::from_dynamic_image
        let raw_frame = RawImageFrame::from_image(dynamic_image);
        let still_image =
            StillImage::from_raw_frame(raw_frame, [image_width, image_height], texture_manager);

        Ok(Image::Still(still_image))
    }

    fn load_jpeg_ls_image(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let file = File::open(file)?;

        let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(file));
        let pixels = decoder.decode()?;
        let info = decoder.info().ok_or_else(|| "Failed to get image info")?;

        let image = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
            info.width as u32,
            info.height as u32,
            pixels,
        )
        .ok_or_else(|| "Failed to create image buffer")?;

        let dynamic_image = image::DynamicImage::ImageRgb8(image);

        let image_width = dynamic_image.width() as usize;
        let image_height = dynamic_image.height() as usize;

        let raw_frame = RawImageFrame::from_image(dynamic_image);
        let still_image =
            StillImage::from_raw_frame(raw_frame, [image_width, image_height], texture_manager);

        Ok(Image::Still(still_image))
    }

    fn load_jbig_image(
        texture_manager: &SharedTextureManager,
        file: &PathBuf,
    ) -> Result<Image, Box<dyn std::error::Error>> {
        let doc = jbig2dec::Document::open(file)?;

        let mut images: Vec<Image> = Vec::new();

        for image in doc.images() {
            let width = image.width();
            let height = image.height();
            let data = image.data().to_vec();

            let image = Image::Still(StillImage::from_pixels(
                data,
                [width as usize, height as usize],
                texture_manager,
            ));

            images.push(image);
        }

        Ok(images.pop().unwrap())
    }
}
