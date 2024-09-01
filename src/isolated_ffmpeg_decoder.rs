mod utils;

use std::env;
use std::{io::Write, path::PathBuf};

use ffmpeg_next::{
    self as ffmpeg,
    codec::context::Context as CodecContext,
    format::{self, pixel::Pixel},
    media::Type::Video as VideoType,
    software::scaling::{context::Context as ScalingContext, flag::Flags},
    util::frame::video::Video as VideoFrame,
};
use interprocess::local_socket::prelude::LocalSocketListener;
use interprocess::local_socket::{Listener, ToFsName};
use interprocess::os::windows::local_socket::NamedPipe;
use utils::calculate_contain_size;

const SHM_NAME: &str = "my_shared_memory";
const SEM_NAME: &str = "my_semaphore";
const BUFFER_SIZE: usize = 1024;

#[cfg(unix)]
const SOCKET_PATH: &str = "/tmp/rust-ipc.sock";

#[cfg(windows)]
const SOCKET_PATH: &str = r"\\.\pipe\rust-ipc";

fn main() {
    ffmpeg::init().unwrap();

    let args = env::args().collect::<Vec<String>>();
    let path = PathBuf::from(&args[1]);
    let size = args[2].parse::<f32>().unwrap();

    let socket_name = SOCKET_PATH.to_fs_name::<NamedPipe>().unwrap();
    //let listener = LocalSocketListener::bind(socket_name).unwrap();

    // check path and size
    if !path.exists() {
        return;
    }

    if size <= 0.0 {
        return;
    }

    let data = load_thumbnail_ffmpeg(&path, size).unwrap();

    println!("Thumbnail data size: {}", data.len());
}

fn load_thumbnail_ffmpeg(file: &PathBuf, size: f32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut ictx = format::input(file)?;

    let input = ictx
        .streams()
        .best(VideoType)
        .ok_or("No video stream found")?;

    let video_stream_index = input.index();
    let context = CodecContext::from_parameters(input.parameters())?;
    let mut decoder = context.decoder().video()?;

    let thumbnail_size =
        calculate_contain_size(size, size, decoder.width() as f32, decoder.height() as f32);

    let mut scaler = ScalingContext::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGBA,
        thumbnail_size.0.trunc() as u32,
        thumbnail_size.1.trunc() as u32,
        Flags::BILINEAR,
    )?;

    for (stream, packet) in ictx.packets() {
        if stream.index() == video_stream_index {
            decoder.send_packet(&packet)?;

            let mut frame = VideoFrame::empty();
            decoder.receive_frame(&mut frame)?;

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

            return Ok(buffer);
        }
    }

    Err("No frames found".into())
}
