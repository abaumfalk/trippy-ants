//! Trippy Ants.
//!
//! A visually attractive simulation based on cellular automata and particle systems.
//!
//! This is the main entry point for the simulation.
//!
//! It creates the window, initializes the simulation, and runs the main loop.

#![warn(clippy::all, clippy::pedantic)]
// #![allow(
//     clippy::cast_lossless,
//     clippy::cast_possible_truncation,
//     clippy::cast_sign_loss,
//     clippy::cast_precision_loss,
//     clippy::cast_possible_wrap
// )]

mod agent;
mod config;
mod frame;
mod grid;
mod palette;
mod random;
mod simulation;

use chrono::Local;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use rayon::iter::{IntoParallelRefMutIterator as _, ParallelIterator as _};
use std::{
    iter,
    path::Path,
    time::{Duration, Instant},
};

use crate::{
    agent::Agent, config::ACTIVE_CONFIG, frame::Frame, palette::Palette, simulation::Simulation,
};

/// Width of the simulation and frame buffer in pixels.
const WIDTH: u16 = 1920;

/// Height of the simulation and frame buffer in pixels.
const HEIGHT: u16 = 1080;

/// Maximum framerate for displaying updates.
/// This saves on CPU for the actual computation.
const MAX_FPS: u64 = 30;

/// Start the application.
///
/// # Panics
///
/// Panics if the window cannot be created.
fn main() {
    let config = ACTIVE_CONFIG;
    let mut rng = 0xfeed_face_u32;

    let palette = Palette::<1024>::new(&config.colors);

    let mut window = Window::new(
        "Trippy Ants (Space: save screenshot, Esc: quit)",
        usize::from(WIDTH),
        usize::from(HEIGHT),
        WindowOptions {
            resize: false,
            scale: minifb::Scale::X1,
            ..WindowOptions::default()
        },
    )
    .expect("window");

    window.set_target_fps(0); // no sleep between polls — FPS reflects CPU fire + blit cost

    let mut frames_in_window = 0_u32;
    let mut window_start = Instant::now();

    let mut buffer = Simulation::new(WIDTH, HEIGHT, &config);
    let mut frame = Frame::new(WIDTH, HEIGHT);
    let mut agents = iter::repeat_with(|| Agent::new(&config, WIDTH, HEIGHT, &mut rng))
        .take(config.agent.count)
        .collect::<Vec<_>>();

    let mut frame_timeout = Instant::now();
    while window.is_open() && !window.is_key_pressed(Key::Escape, KeyRepeat::No) {
        buffer.swap_buffers();
        buffer.blur();

        // limit display framerate
        if frame_timeout.elapsed() >= Duration::from_millis(1000 / MAX_FPS) {
            frame.update(&buffer.write_buffer, &palette);
            frame_timeout = Instant::now();
        }
        agents.par_iter_mut().for_each(|agent| {
            agent.update(&buffer);
        });
        buffer.update(&agents);

        frame.update_window(&mut window);

        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            let filename = format!(
                "trippy-ants_{}.png",
                Local::now().format("%Y-%m-%d_%H-%M-%S")
            );
            match frame.save_png(Path::new(&filename)) {
                Ok(()) => println!("saved {filename}"),
                Err(error) => eprintln!("failed to save {filename}: {error}"),
            }
        }

        frames_in_window += 1;
        let elapsed = window_start.elapsed();
        if elapsed.as_secs_f64() >= 1.0 {
            let fps = f64::from(frames_in_window) / elapsed.as_secs_f64();
            println!("{fps:.1} FPS");
            frames_in_window = 0;
            window_start += Duration::from_secs(1);
        }
    }
}
