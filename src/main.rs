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
use std::{
    cmp::Ordering,
    collections::VecDeque,
    env, mem,
    path::Path,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    agent::Agent,
    config::{Config, ConfigWatcher, DEFAULT_CONFIG},
    frame::Frame,
    grid::Grid,
    palette::Palette,
    simulation::Simulation,
};

/// Width of the simulation and frame buffer in pixels.
const WIDTH: u16 = 1920;

/// Height of the simulation and frame buffer in pixels.
const HEIGHT: u16 = 1080;

/// Maximum framerate for displaying updates.
const MAX_FPS: u64 = 30;

/// Maximum Number of TPS (Time Per Simulation-Step) samples to keep around.
const TPS_HISTORY_MAX: usize = 64_000;

/// Time per rendered frame.
#[expect(clippy::cast_possible_truncation, reason = "this is a const...")]
const FRAME_TIME: Duration =
    Duration::from_nanos(Duration::from_secs(1).as_nanos() as u64 / MAX_FPS);

/// Profiling timings for the simulation loop.
#[derive(Default, Debug)]
struct Timings {
    /// Time spent updating the grid state.
    grid_update: Duration,
    /// Time spent processing configuration updates.
    config_update: Duration,
    /// Time spent swapping buffers.
    swap: Duration,
    /// Time spent on blur simulation.
    blur: Duration,
    /// Time spent on agent simulation updates.
    agents: Duration,
    /// Time spent applying agents to the grid.
    apply_agents: Duration,
    /// Time spent on boundary condition application.
    bc: Duration,
    /// Time spent in synchronization phase.
    sync: Duration,
}

impl Timings {
    /// Resets all timing accumulators to zero.
    fn reset(&mut self) {
        *self = Self::default();
    }

    /// Prints the current profiling statistics to stdout.
    fn print(&self, elapsed: Duration, mean: f64, median: f64, stddev: f64) {
        let total_secs = elapsed.as_secs_f64();
        let pct_blur = (self.blur.as_secs_f64() / total_secs) * 100.0;
        let pct_agents = (self.agents.as_secs_f64() / total_secs) * 100.0;
        let pct_apply_agents = (self.apply_agents.as_secs_f64() / total_secs) * 100.0;
        let pct_grid = (self.grid_update.as_secs_f64() / total_secs) * 100.0;
        let pct_sync = (self.sync.as_secs_f64() / total_secs) * 100.0;

        let median_fps = 1e6 / median;

        let pct_rem = pct_grid + pct_sync;

        println!(
            "Mean: {mean:>6.1} | Median: {median:>6.1} | StdDev: {stddev:>6.1} | Median FPS: {median_fps:6.1} | Blur: {pct_blur:>4.1}% | Agent: {pct_agents:>4.1}% | Apply: {pct_apply_agents:>4.1}% | Remainder: {pct_rem:>4.1}%"
        );
    }
}

/// Simulation thread controller.
struct Simulator<'sim> {
    /// The simulation world state.
    simulation: Simulation,
    /// The collection of agents in the simulation.
    agents: Vec<Agent>,
    /// Thread-safe flag to signal if the simulation should continue running.
    is_running: &'sim AtomicBool,
    /// Channel to receive the old grid from the renderer.
    render_to_sim_rx: mpsc::Receiver<Grid>,
    /// Channel to send the current read grid back to the renderer.
    sim_to_render_tx: mpsc::Sender<Grid>,
    /// Channel to receive configuration updates from the main thread.
    config_rx: mpsc::Receiver<Config>,
    /// Profiling timings accumulator.
    timings: Timings,
}

impl<'sim> Simulator<'sim> {
    /// Creates a new Simulator instance.
    fn new(
        simulation: Simulation,
        agents: Vec<Agent>,
        is_running: &'sim AtomicBool,
        render_to_sim_rx: mpsc::Receiver<Grid>,
        sim_to_render_tx: mpsc::Sender<Grid>,
        config_rx: mpsc::Receiver<Config>,
    ) -> Self {
        Self {
            simulation,
            agents,
            is_running,
            render_to_sim_rx,
            sim_to_render_tx,
            config_rx,
            timings: Timings::default(),
        }
    }

    /// Update all agents.
    fn update_agents(&mut self) {
        let total_agents = self.agents.len();

        if total_agents == 0 {
            return;
        }

        // Split the borrow of `self` so we can pass the immutable simulation
        // and the mutable agent chunks into the closures safely.
        let simulation = &self.simulation;
        let mut remaining_agents = self.agents.as_mut_slice();

        rayon::scope(|scope| {
            let num_workers = rayon::current_num_threads();
            let agents_per_worker = total_agents / num_workers;
            let remainder = total_agents % num_workers;

            for i in 0..num_workers {
                // Distribute any remainder agents across the first few chunks
                let agents_for_this_worker = agents_per_worker + usize::from(i < remainder);

                if agents_for_this_worker == 0 {
                    continue;
                }

                // Safely split the mutable slice
                let (chunk, rest) = remaining_agents.split_at_mut(agents_for_this_worker);
                remaining_agents = rest;

                // Spawn all chunks directly into the threadpool
                scope.spawn(move |_| {
                    for agent in chunk {
                        agent.update(simulation);
                    }
                });
            }
        });
    }

    /// Runs the main simulation loop.
    ///
    /// # Panics
    /// If the render thread has exited.
    fn run(mut self) {
        let mut last_sps_calculation = Instant::now();
        let mut step_durations = VecDeque::with_capacity(TPS_HISTORY_MAX);
        let mut median_buffer = Vec::with_capacity(TPS_HISTORY_MAX);

        while self.is_running.load(AtomicOrdering::Relaxed) {
            let step_start = Instant::now();

            // 1. Sync Phase
            let t_sync = Instant::now();

            // If the renderer requested a frame update, it sent us its old render_grid.
            if let Ok(mut renderer_grid) = self.render_to_sim_rx.try_recv() {
                // Swap the renderer's buffer with our current read_buffer.
                // Our old read_buffer goes into `renderer_grid`.
                mem::swap(&mut self.simulation.read_buffer, &mut renderer_grid);

                // Send the old read_buffer to the UI thread for rendering.
                self.sim_to_render_tx
                    .send(renderer_grid)
                    .expect("Failed to send the updated state to the render thread");
            }
            self.timings.sync += t_sync.elapsed();

            // Swap buffers
            // If we switched buffers with the UI thread,
            // this naturally turns the renderer's old buffer (now in read_buffer)
            // into the new write_buffer for the upcoming simulation step.
            let t_start = Instant::now();
            self.simulation.swap_buffers();
            self.timings.swap += t_start.elapsed();

            // Process Config Updates
            let t_config = Instant::now();
            while let Ok(new_config) = self.config_rx.try_recv() {
                for (index, agent) in self.agents.iter_mut().enumerate() {
                    let index = u32::try_from(index).unwrap_or(u32::MAX);
                    agent.update_config(&new_config.agent, index);
                }
                self.simulation.update_config(&new_config.world);

                while self.agents.len() < new_config.agent.count as usize {
                    let index = u32::try_from(self.agents.len()).unwrap_or(u32::MAX);
                    self.agents
                        .push(Agent::new(&new_config, WIDTH, HEIGHT, index));
                }
                self.agents.truncate(new_config.agent.count as usize);
            }
            self.timings.config_update += t_config.elapsed();

            // Simulate Next Step
            let t_blur = Instant::now();
            self.simulation.blur();
            self.timings.blur += t_blur.elapsed();

            let t_agents = Instant::now();
            self.update_agents();
            self.timings.agents += t_agents.elapsed();

            let t_apply_agents = Instant::now();
            self.simulation.apply_agents(&self.agents);
            self.timings.apply_agents += t_apply_agents.elapsed();

            let t_bc = Instant::now();
            self.simulation.apply_bc();
            self.timings.bc += t_bc.elapsed();

            // Track duration of this step
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
                let median = if median_buffer.len().is_multiple_of(2) {
                    median_buffer[i_mid - 1].midpoint(median_buffer[i_mid])
                } else {
                    median_buffer[i_mid]
                };

                self.timings.print(elapsed, mean, median, stddev);
                self.timings.reset();

                last_sps_calculation += Duration::from_secs(1);
            }
        }
    }
}

fn main() -> ExitCode {
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
        DEFAULT_CONFIG
    };

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
    window.set_target_fps(0);

    let simulation = Simulation::new(WIDTH, HEIGHT, &config.world);
    let render_grid = simulation.make_scratch_grid();

    let agents = (0..config.agent.count)
        .map(|index| Agent::new(&config, WIDTH, HEIGHT, index))
        .collect::<Vec<_>>();

    let is_running = AtomicBool::new(true);
    let (render_to_sim_tx, render_to_sim_rx) = mpsc::channel::<Grid>();
    let (sim_to_render_tx, sim_to_render_rx) = mpsc::channel::<Grid>();
    let (config_tx, config_rx) = mpsc::channel::<Config>();

    let mut frame = Frame::new(WIDTH, HEIGHT);
    let mut render_grid_opt = Some(render_grid);
    let mut frame_timeout = Instant::now() + FRAME_TIME;

    thread::scope(|scope| {
        let is_running_ref = &is_running;
        let _sim_thread = thread::Builder::new()
            .name("sim_worker_0".to_owned())
            .spawn_scoped(scope, move || {
                Simulator::new(
                    simulation,
                    agents,
                    is_running_ref,
                    render_to_sim_rx,
                    sim_to_render_tx,
                    config_rx,
                )
                .run();
            });

        while window.is_open() && !window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            if Instant::now() >= frame_timeout {
                if let Some(grid) = render_grid_opt.take() {
                    render_to_sim_tx
                        .send(grid)
                        .expect("Unexpected channel closure");
                }
                if let Ok(new_grid) = sim_to_render_rx.try_recv() {
                    render_grid_opt = Some(new_grid);
                }

                if let Some(grid) = render_grid_opt.as_ref() {
                    frame.update(grid, &palette);
                    frame.update_window(&mut window);
                }

                if window.is_key_pressed(Key::Space, KeyRepeat::No) {
                    let filename = format!(
                        "trippy-ants_{}.png",
                        Local::now().format("%Y-%m-%d_%H-%M-%S")
                    );
                    frame
                        .save_png(Path::new(&filename))
                        .expect("Failed to save png");
                }

                let now = Instant::now();
                while frame_timeout <= now {
                    frame_timeout += FRAME_TIME;
                }

                if let Some(new_config) = config_watcher.watch_for_update() {
                    palette = Palette::<1024>::new(&new_config.colors);
                    config_tx
                        .send(new_config)
                        .expect("Unexpected channel closure");
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        is_running.store(false, AtomicOrdering::Relaxed);
    });
    ExitCode::SUCCESS
}
