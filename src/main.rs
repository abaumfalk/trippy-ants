#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

mod agent;
mod config;
mod frame;
mod grid;
mod random;

use chrono::Local;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use std::{
    path::Path,
    time::{Duration, Instant},
};

use crate::{
    agent::Agent,
    config::DEFAULT_CONFIG,
    frame::{Frame, Palette},
    grid::Simulation,
};

const WIDTH: usize = 1920;
const HEIGHT: usize = 1080;

/// [`Frame::pixels`] / minifb: 0x00RRGGBB per pixel, row-major.
fn save_png(
    pixels: &[u32],
    width: usize,
    height: usize,
    path: &Path,
) -> Result<(), image::ImageError> {
    let mut rgb = Vec::with_capacity(width * height * 3);
    for px in pixels {
        rgb.push(((px >> 16) & 0xFF) as u8);
        rgb.push(((px >> 8) & 0xFF) as u8);
        rgb.push((px & 0xFF) as u8);
    }
    image::save_buffer(
        path,
        &rgb,
        width as u32,
        height as u32,
        image::ColorType::Rgb8,
    )
}

fn main() {
    let config = DEFAULT_CONFIG;
    let mut rng = 0xfeed_face_u32;

    let palette = Palette::new(config.limit);

    let mut window = Window::new(
        "Trippy Ants (Space: save screenshot, Esc: quit)",
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: false,
            scale: minifb::Scale::X1,
            ..WindowOptions::default()
        },
    )
    .expect("window");

    window.set_target_fps(0); // no sleep between polls — FPS reflects CPU fire + blit cost

    let mut frames_in_window = 0u32;
    let mut window_start = Instant::now();

    let mut buffer = Simulation::new(
        WIDTH,
        HEIGHT,
        config.limit,
        config.enable_walls,
        config.grid_topology,
    );
    let mut frame = Frame::new(WIDTH, HEIGHT, palette);
    let mut agents = (0..config.agent_count)
        .map(|_| Agent::new(&config, WIDTH, HEIGHT, &mut rng))
        .collect::<Vec<_>>();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        buffer.blur(config.decay_factor, config.grid_topology);
        agents.par_iter_mut().for_each(|agent| {
            agent.update(&buffer);
        });
        buffer.update(&mut agents);

        frame.update(&buffer.read_buffer);
        window
            .update_with_buffer(&frame.pixels, WIDTH, HEIGHT)
            .expect("update");

        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            let filename = format!(
                "trippy-ants_{}.png",
                Local::now().format("%Y-%m-%d_%H-%M-%S")
            );
            match save_png(&frame.pixels, WIDTH, HEIGHT, Path::new(&filename)) {
                Ok(()) => println!("saved {filename}"),
                Err(e) => eprintln!("failed to save {filename}: {e}"),
            }
        }

        frames_in_window += 1;
        let elapsed = window_start.elapsed();
        if elapsed.as_secs_f64() >= 1.0 {
            let fps = frames_in_window as f64 / elapsed.as_secs_f64();
            println!("{fps:.1} FPS");
            frames_in_window = 0;
            window_start += Duration::from_secs(1);
        }
    }
}
