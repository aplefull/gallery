use crate::{measure_time, utils::load_texture};
use eframe::{
    egui::{self, mutex::RwLock, Color32, ColorImage, TextureHandle},
    epaint::TextureManager,
};
use ffmpeg_next::{
    codec::context::Context as CodecContext,
    decoder::{Audio as AudioDecoder, Video as VideoDecoder},
    format::{self, context::Input as InputContext, Pixel},
    frame::Audio as AudioFrame,
    frame::Video as VideoFrame,
    media::Type::{Audio as AudioType, Video as VideoType},
    software::scaling::{context::Context as ScalingContext, flag::Flags},
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct FramesBuffer {
    frames: VecDeque<VideoFrame>,
    size: usize,
}

impl FramesBuffer {
    pub fn new() -> Self {
        Self {
            frames: VecDeque::new(),
            size: 5,
        }
    }

    pub fn push(&mut self, frame: VideoFrame) {
        self.frames.push_back(frame);
    }

    pub fn pop(&mut self) -> Option<VideoFrame> {
        self.frames.pop_front()
    }

    pub fn front(&mut self) -> Option<VideoFrame> {
        self.frames.front().cloned()
    }

    pub fn clear(&mut self) {
        self.frames.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn should_fill_buffer(&self) -> bool {
        self.frames.len() < self.size
    }
}

pub struct VideoEntry {
    pub path: PathBuf,
    pub video_decoder: VideoDecoder,
    pub audio_decoder: AudioDecoder,
    pub scaler: ScalingContext,
    pub video_input_ctx: InputContext,
    pub audio_input_ctx: InputContext,
    pub video_stream_index: usize,
    pub audio_stream_index: usize,
    pub audio_sink: rodio::Sink,
    pub audio_playback_stream: rodio::OutputStream,
    pub audio_stream_handle: rodio::OutputStreamHandle,
    pub frames: Arc<Mutex<Vec<egui::TextureHandle>>>,
    pub frame_rate: f64,
    pub last_frame_time: Instant,
    pub current_frame_index: usize,
    pub current_time: u64,
    pub video_duration: u64,
    pub is_playing: bool,
    cached_frame: Option<egui::TextureHandle>,
    eof_reached: bool,
    frames_buffer: FramesBuffer,
}

#[inline]
fn video_frame_to_image(frame: VideoFrame) -> ColorImage {
    let size = [frame.width() as usize, frame.height() as usize];
    let data = frame.data(0);
    let stride = frame.stride(0);
    let pixel_size_bytes = 4;
    let width: usize = pixel_size_bytes * frame.width() as usize;
    let height: usize = frame.height() as usize;
    let mut pixels = Vec::new();

    for line in 0..height {
        let start = line * stride;
        let end = start + width;
        let row = &data[start..end];

        pixels.extend(
            row.chunks_exact(pixel_size_bytes)
                .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])),
        )
    }

    ColorImage { size, pixels }
}

pub fn video_frame_to_texture(frame: VideoFrame, ctx: &egui::Context) -> Option<TextureHandle> {
    let texture_manager = ctx.tex_manager();
    let color_image = video_frame_to_image(frame);

    Some(load_texture(texture_manager, color_image))
}
impl VideoEntry {
    pub fn new(video_path: &PathBuf) -> Option<Self> {
        let video_input_ctx = match format::input(&video_path) {
            Ok(ictx) => ictx,
            Err(err) => {
                println!("Error opening video file: {:?}", err);

                return None;
            }
        };

        let audio_input_ctx = match format::input(&video_path) {
            Ok(ictx) => ictx,
            Err(err) => {
                println!("Error opening video file: {:?}", err);

                return None;
            }
        };

        let streams = video_input_ctx.streams();

        let video_stream = match streams.best(VideoType) {
            Some(stream) => stream,
            None => {
                println!("No video stream found in file: {:?}", video_path);

                return None;
            }
        };

        let audio_stream = match streams.best(AudioType) {
            Some(stream) => stream,
            None => {
                println!("No audio stream found in file: {:?}", video_path);

                return None;
            }
        };

        let video_decoder_ctx = match CodecContext::from_parameters(video_stream.parameters()) {
            Ok(context) => context,
            Err(err) => {
                println!("Error creating codec context: {:?}", err);

                return None;
            }
        };

        let audio_decoder_ctx = match CodecContext::from_parameters(audio_stream.parameters()) {
            Ok(context) => context,
            Err(err) => {
                println!("Error creating audio codec context: {:?}", err);

                return None;
            }
        };

        let video_decoder = match video_decoder_ctx.decoder().video() {
            Ok(decoder) => decoder,
            Err(err) => {
                println!("Error creating video decoder: {:?}", err);

                return None;
            }
        };

        let audio_decoder = match audio_decoder_ctx.decoder().audio() {
            Ok(decoder) => decoder,
            Err(err) => {
                println!("Error creating audio decoder: {:?}", err);

                return None;
            }
        };

        let scaler = match ScalingContext::get(
            video_decoder.format(),
            video_decoder.width(),
            video_decoder.height(),
            Pixel::RGBA,
            video_decoder.width(),
            video_decoder.height(),
            Flags::BILINEAR,
        ) {
            Ok(scaler) => scaler,
            Err(err) => {
                println!("Error creating scaler context: {:?}", err);

                return None;
            }
        };

        let (stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&stream_handle).unwrap();

        let frame_rate =
            video_stream.avg_frame_rate().0 as f64 / video_stream.avg_frame_rate().1 as f64;

        let video_duration = match video_stream.duration() {
            duration if duration >= 0 => {
                let time_base = f64::from(video_stream.time_base());
                (duration as f64 * time_base * 1000.0).round() as u64
            }
            _ => match video_input_ctx.duration() {
                duration if duration >= 0 => (duration as f64 / 1000.0).round() as u64,
                _ => {
                    println!("Warning: Could not determine video duration");

                    0
                }
            },
        };

        let mut entry = VideoEntry {
            current_time: 0,
            video_duration,
            path: video_path.clone(),
            video_decoder,
            audio_decoder,
            scaler,
            video_stream_index: video_stream.index(),
            audio_stream_index: audio_stream.index(),
            video_input_ctx,
            audio_input_ctx,
            audio_sink: sink,
            audio_playback_stream: stream,
            audio_stream_handle: stream_handle,
            frames: Arc::new(Mutex::new(Vec::new())),
            frame_rate,
            last_frame_time: Instant::now(),
            current_frame_index: 0,
            eof_reached: false,
            frames_buffer: FramesBuffer::new(),
            is_playing: false,
            cached_frame: None,
        };

        entry.decode_next_audio_packet();

        Some(entry)
    }

    pub fn decode_next_audio_packet(&mut self) {
        let receive_and_process_decoded_audio = |decoder: &mut AudioDecoder| {
            let mut decoded = AudioFrame::empty();

            while decoder.receive_frame(&mut decoded).is_ok() {
                let samples: Vec<f32> = decoded.plane::<f32>(0).to_vec();
                return Some(samples);
            }

            None
        };

        let sample_rate = self.audio_decoder.rate();
        let channel_count = self.audio_decoder.channel_layout().channels();

        for (stream, packet) in self.audio_input_ctx.packets() {
            if stream.index() == self.audio_stream_index {
                match self.audio_decoder.send_packet(&packet) {
                    Ok(_) => {}
                    Err(err) => {
                        println!("Error sending audio packet to decoder: {:?}", err);
                    }
                };

                if let Some(samples) = receive_and_process_decoded_audio(&mut self.audio_decoder) {
                    let source = rodio::buffer::SamplesBuffer::new(
                        channel_count as u16,
                        sample_rate / channel_count as u32,
                        samples,
                    );

                    self.audio_sink.append(source);
                }
            }
        }
    }

    fn decode_next_frame(&mut self) -> Option<VideoFrame> {
        let mut receive_and_process_decoded_frames = |decoder: &mut VideoDecoder| {
            let mut decoded = VideoFrame::empty();

            while decoder.receive_frame(&mut decoded).is_ok() {
                let mut frame = VideoFrame::empty();

                match self.scaler.run(&decoded, &mut frame) {
                    Ok(_) => {}
                    Err(err) => {
                        println!("Error scaling frame: {:?}", err);

                        return None;
                    }
                };

                return Some(frame);
            }

            None
        };

        for (stream, packet) in self.video_input_ctx.packets() {
            if stream.index() == self.video_stream_index {
                let current_pts = packet.pts().unwrap_or(0);

                match self.video_decoder.send_packet(&packet) {
                    Ok(_) => {}
                    Err(err) => {
                        println!("Error sending packet to decoder: {:?}", err);

                        return None;
                    }
                };

                if let Some(frame) = receive_and_process_decoded_frames(&mut self.video_decoder) {
                    self.current_time =
                        (current_pts as f64 * f64::from(stream.time_base()) * 1000.0).round()
                            as u64;
                    self.current_time = self.current_time.min(self.video_duration);
                    return Some(frame);
                } else {
                    //self.eof_reached = true;
                    //self.video_decoder.send_eof().unwrap();

                    return None;
                }
            }
        }

        //self.eof_reached = true;
        self.video_decoder.send_eof().unwrap();
        None
    }

    pub fn get_current_frame(&mut self, ctx: &egui::Context) -> Option<TextureHandle> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame_time).as_secs_f64();
        let texture_handle;

        if !self.is_playing {
            if self.frames_buffer.is_empty() {
                let frame = self.decode_next_frame();

                match frame {
                    Some(frame) => self.frames_buffer.push(frame),
                    None => {}
                }
            }

            texture_handle = match self.cached_frame {
                Some(ref tex) => Some(tex.clone()),
                None => match self.frames_buffer.front() {
                    Some(frame) => video_frame_to_texture(frame, ctx),
                    None => None,
                },
            };

            self.cached_frame = texture_handle.clone();
            self.audio_sink.pause();

            return texture_handle;
        }

        if elapsed >= 1.0 / self.frame_rate {
            self.last_frame_time = now;

            while self.frames_buffer.should_fill_buffer() {
                let frame = self.decode_next_frame();

                match frame {
                    Some(frame) => self.frames_buffer.push(frame),
                    None => {
                        break;
                    }
                }
            }

            texture_handle = match self.frames_buffer.pop() {
                Some(frame) => video_frame_to_texture(frame, ctx),
                None => None,
            };
        } else {
            texture_handle = match self.cached_frame {
                Some(ref tex) => Some(tex.clone()),
                None => match self.frames_buffer.front() {
                    Some(frame) => video_frame_to_texture(frame, ctx),
                    None => None,
                },
            };
        }

        if self.frames_buffer.is_empty() {
            self.audio_sink.pause();
        } else {
            self.audio_sink.play();
        }

        let audio_pos = self.audio_sink.get_pos().as_millis() as u64;
        let video_pos = self.current_time;

        println!("Video pos: {}, Audio pos: {}", video_pos, audio_pos);

        /*   if (audio_pos as i64 - video_pos as i64).abs() > 60 {
            match self.audio_sink.try_seek(Duration::from_millis(video_pos)) {
                Ok(_) => {}
                Err(err) => {
                    println!("Error seeking audio: {:?}", err);
                }
            }
        } */

        self.cached_frame = texture_handle.clone();

        ctx.request_repaint();

        texture_handle
    }

    pub fn play(&mut self) {
        self.is_playing = true;
    }

    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    pub fn toggle_playback(&mut self) {
        if self.is_playing {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn seek(&mut self, time: u64) {
        let time = time.min(self.video_duration);
        let time = time.max(0);

        let stream = self.video_input_ctx.streams().best(VideoType).unwrap();
        let time_base = f64::from(stream.time_base());
        let pts = (time as f64 / (time_base * 1000.0)) as i64;

        match self.video_input_ctx.seek(pts, 0..i64::MAX) {
            Ok(_) => {}
            Err(err) => {
                println!("Error seeking video: {:?}", err);
            }
        }

        match self.audio_sink.try_seek(Duration::from_millis(time)) {
            Ok(_) => {}
            Err(err) => {
                println!("Error seeking audio: {:?}", err);
            }
        }

        self.frames_buffer.clear();
        self.current_time = time;
    }

    pub fn seek_relative(&mut self, time: i64) {
        let new_time = self.current_time as i64 + time;

        self.seek(new_time as u64);
    }
}
