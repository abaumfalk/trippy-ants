//! Trippy Ants.
//!
//! A visually attractive simulation based on cellular automata and particle systems.
//!
//! This is the main entry point for the simulation.
//!
//! It creates the window, initializes the simulation, and runs the main loop.

#![warn(clippy::all, clippy::pedantic)]

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
    cmp::Ordering,
    collections::VecDeque,
    env,
    path::Path,
    process::ExitCode,
    time::{Duration, Instant},
};
use toml::ser;

use crate::{
    agent::Agent,
    config::{ConfigWatcher, DEFAULT_CONFIG},
    frame::Frame,
    palette::Palette,
    simulation::Simulation,
};

/// Width of the simulation and frame buffer in pixels.
const WIDTH: u16 = 1920;

/// Height of the simulation and frame buffer in pixels.
const HEIGHT: u16 = 1080;

/// Maximum framerate for displaying updates.
/// This saves on CPU for the actual computation.
const MAX_FPS: u64 = 30;

/// Maximum Number of TPS (Time Per Simulation-Step) samples to keep around.
const TPS_HISTORY_MAX: usize = 64_000;

/// Start the application.
///
/// # Panics
///
/// Panics if the window cannot be created.
#[expect(clippy::too_many_lines, reason = "Function is still understandable")]
fn main() -> ExitCode {
    // read path to config file from command line

    let mut config_watcher = ConfigWatcher::new();
    let config = if let Some(config_file) = env::args().nth(1) {
        match config_watcher.load_config(config_file) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        println!("no config file provided, using default config");
        DEFAULT_CONFIG
    };

    if let Ok(config_str) = ser::to_string(&config) {
        println!("loaded config:\n{config_str}");
    }

    let mut palette = Palette::<1024>::new(&config.colors);

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

    let mut simulation = Simulation::new(WIDTH, HEIGHT, &config.world);
    let mut frame = Frame::new(WIDTH, HEIGHT);
    let mut agents = (0..config.agent.count)
        .map(|index| {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            Agent::new(&config, WIDTH, HEIGHT, index)
        })
        .collect::<Vec<_>>();

    let mut frame_timeout = Instant::now();
    let mut last_sps_calculation = Instant::now();
    let mut step_durations = VecDeque::with_capacity(TPS_HISTORY_MAX);
    let mut median_buffer = Vec::with_capacity(TPS_HISTORY_MAX);
    while window.is_open() && !window.is_key_pressed(Key::Escape, KeyRepeat::No) {
        let step_start = Instant::now();
        simulation.swap_buffers();
        simulation.blur();

        // limit display framerate
        if frame_timeout.elapsed() >= Duration::from_millis(1000 / MAX_FPS) {
            frame.update(&simulation.write_buffer, &palette);
            frame.update_window(&mut window);
            frame_timeout = Instant::now();
        }
        agents.par_iter_mut().for_each(|agent| {
            agent.update(&simulation);
        });
        simulation.update(&agents);

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

        if step_durations.len() >= TPS_HISTORY_MAX {
            _ = step_durations.pop_front();
        }
        step_durations.push_back(step_start.elapsed());

        let elapsed = last_sps_calculation.elapsed();
        if elapsed.as_secs_f64() >= 1.0 {
            median_buffer.clear();
            median_buffer.extend(
                step_durations
                    .iter()
                    .map(|sample| sample.as_secs_f64() * 1e6),
            );

            #[expect(
                clippy::cast_precision_loss,
                reason = "a buffer this large would not fit any memory"
            )]
            let count = median_buffer.len() as f64;
            let sum: f64 = median_buffer.iter().sum();
            let mean = sum / count;

            let variance = median_buffer
                .iter()
                .map(|sample| (sample - mean).powi(2))
                .sum::<f64>()
                / count;
            let stddev = variance.sqrt();

            #[expect(clippy::min_ident_chars, reason = "these names are fine")]
            median_buffer.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Less));

            let i_mid = median_buffer.len() / 2;
            #[expect(clippy::indexing_slicing, reason = "checked above")]
            let median = if median_buffer.len() % 2 == 0 {
                f64::midpoint(median_buffer[i_mid - 1], median_buffer[i_mid])
            } else {
                median_buffer[i_mid]
            };

            let median_fps = 1e6 / median;
            println!("Mean: {mean:>6.1} | Median: {median:>6.1} | StdDev: {stddev:>6.1} | Median FPS: {median_fps:6.1}");
            last_sps_calculation += Duration::from_secs(1);
        }

        if let Some(new_config) = config_watcher.watch_for_update() {
            println!("config updated");
            if let Ok(config_str) = ser::to_string(&new_config) {
                println!("loaded config:\n{config_str}");
            }
            for (index, agent) in agents.iter_mut().enumerate() {
                let index = u32::try_from(index).unwrap_or(u32::MAX);
                agent.update_config(&new_config.agent, index);
            }
            simulation.update_config(&new_config.world);

            palette = Palette::<1024>::new(&new_config.colors);

            while agents.len() < new_config.agent.count {
                let index = u32::try_from(agents.len()).unwrap_or(u32::MAX);
                agents.push(Agent::new(&new_config, WIDTH, HEIGHT, index));
            }
            agents.truncate(new_config.agent.count);
        }
    }

    ExitCode::SUCCESS
}
