use eframe::{
    egui::{self, mutex::RwLock, ColorImage, TextureHandle},
    epaint::TextureManager,
};
use std::{path::PathBuf, sync::Arc};

pub type SharedTextureManager = Arc<RwLock<TextureManager>>;

pub fn filter_media_files(files: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut filtered_files = Vec::new();

    for file in files {
        if is_image(&file) || is_video(&file) || is_no_extension(&file) {
            filtered_files.push(file);
        }
    }

    filtered_files
}

pub fn process_entries(entries: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for entry in entries {
        if entry.is_dir() {
            let dir_files = get_files_recursive(&entry);
            files.extend(dir_files);
        } else {
            files.push(entry);
        }
    }

    files
}

pub fn filter_valid_paths(paths: Vec<String>) -> Vec<PathBuf> {
    let mut valid_paths = Vec::new();

    for path in paths {
        let path = PathBuf::from(path);
        if path.exists() {
            valid_paths.push(path);
        }
    }

    valid_paths
}

pub fn is_image(file: &PathBuf) -> bool {
    let extensions = [
        "3fr", "arw", "avif", "bmp", "cr2", "crw", "cur", "dcm", "dds", "dng", "erf", "gif", "hdr",
        "heic", "heif", "j2c", "jfif", "jls", "jp2", "jpeg", "jpf", "jpg", "jpm", "kdc", "mdc",
        "mef", "mj2", "mos", "mrw", "nef", "nrw", "orf", "pef", "pgm", "png", "ppm", "raf", "raw",
        "rw2", "sr2", "srf", "srw", "tif", "tiff", "webp", "x3f", "png_", "rpgmvp", "jbg", "jb2",
    ];

    let extension = file
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase();

    extensions.contains(&extension.as_str())
}

pub fn is_video(file: &PathBuf) -> bool {
    let video = [
        "3g2", "3gp", "asf", "avi", "flv", "m2ts", "m4v", "mjpeg", "mkv", "mov", "mp4", "mts",
        "mxf", "rm", "rmvb", "swf", "ts", "vob", "webm", "wmv",
    ];

    let extension = file
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase();

    video.contains(&extension.as_str())
}

pub fn is_no_extension(file: &PathBuf) -> bool {
    file.extension().is_none()
}

pub fn get_files_recursive(path: &PathBuf) -> Vec<PathBuf> {
    let mut queue = vec![path.clone()];

    let mut files = Vec::new();

    while let Some(current_path) = queue.pop() {
        if current_path.is_dir() {
            match std::fs::read_dir(&current_path) {
                Ok(entries) => {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            let entry_path = entry.path();

                            if entry_path.is_dir() {
                                queue.push(entry_path);
                            } else {
                                files.push(entry_path);
                            }
                        }
                    }
                }
                Err(err) => {
                    println!("Error reading directory: {:?}", err);
                }
            }
        } else {
            files.push(current_path);
        }
    }

    files
}

pub fn calculate_thumbnail_layout(
    available_width: f32,
    min_thumbnail_width: f32,
    thumbnail_gap: f32,
    max_columns_count: usize,
) -> (usize, f32) {
    for columns in (1..=max_columns_count).rev() {
        let total_gaps = (columns - 1) as f32 * thumbnail_gap;
        let max_thumbnail_width = (available_width - total_gaps) / columns as f32;

        if max_thumbnail_width >= min_thumbnail_width {
            return (columns, max_thumbnail_width);
        }
    }

    (1, available_width)
}

pub fn calculate_cover_size(
    container_width: f32,
    container_height: f32,
    img_width: f32,
    img_height: f32,
) -> (f32, f32) {
    let container_aspect_ratio = container_width / container_height;
    let img_aspect_ratio = img_width / img_height;

    if img_aspect_ratio > container_aspect_ratio {
        let height = container_height;
        let width = container_height * img_aspect_ratio;
        (width.max(1.0), height.max(1.0))
    } else {
        let width = container_width;
        let height = container_width / img_aspect_ratio;
        (width.max(1.0), height.max(1.0))
    }
}

pub fn calculate_contain_size(
    container_width: f32,
    container_height: f32,
    img_width: f32,
    img_height: f32,
) -> (f32, f32) {
    let container_aspect_ratio = container_width / container_height;
    let img_aspect_ratio = img_width / img_height;

    if img_aspect_ratio > container_aspect_ratio {
        let width = container_width;
        let height = container_width / img_aspect_ratio;
        (width.max(1.0), height.max(1.0))
    } else {
        let height = container_height;
        let width = container_height * img_aspect_ratio;
        (width.max(1.0), height.max(1.0))
    }
}

pub fn get_window_size(ctx: &egui::Context) -> egui::Vec2 {
    match ctx.input(|i| i.viewport().inner_rect) {
        Some(rect) => rect.size(),
        None => egui::vec2(0.0, 0.0),
    }
}

pub fn print_time_elapsed(start: std::time::Instant) {
    let duration = start.elapsed();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let micros = duration.subsec_micros() - millis * 1_000;
    let nanos = duration.subsec_nanos() - millis * 1_000_000 - micros * 1_000;

    println!(
        "Time elapsed: {}s {}ms {}µs {}ns",
        secs, millis, micros, nanos
    );
}

pub fn format_time(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    let seconds = (ms % 60_000) / 1_000;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

pub fn load_texture(texture_manager: SharedTextureManager, image: ColorImage) -> TextureHandle {
    let name = "Texture".to_string();
    let texture_id = texture_manager
        .write()
        .alloc(name, image.into(), Default::default());

    TextureHandle::new(texture_manager, texture_id)
}
