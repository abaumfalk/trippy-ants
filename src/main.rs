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
use pixels::{Pixels, SurfaceTexture};
use std::{
    cmp::Ordering,
    collections::VecDeque,
    env, mem,
    num::NonZero,
    path::Path,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering as AtomicOrdering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use toml::ser;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
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

/// Simulation App state.
/// Contains all state needed to render a frame.
struct App<'app, 'frame, const PALETTE_RES: usize> {
    /// The config watcher, used for live config changes.
    config_watcher: ConfigWatcher,

    /// The Frame which represents the render buffer.
    frame: Option<Frame<'frame>>,

    /// The Window we are rendering to.
    window: Option<Arc<Window>>,

    /// The Window attributes of our window.
    window_attributes: WindowAttributes,

    /// The color palette.
    palette: Palette<PALETTE_RES>,

    /// Flag to signal the simulation thread to terminate.
    is_running: &'app AtomicBool,

    /// Channel to send the current renderer grid back to the sim thread.
    render_to_sim_tx: mpsc::Sender<Grid>,

    /// Channel to receive the new simulation grid state.
    sim_to_render_rx: mpsc::Receiver<Grid>,

    /// Channel to send configuration updates to the simulation thread.
    config_tx: mpsc::Sender<Config>,

    /// Time at which the next frame is due to be rendered.
    frame_timeout: Instant,

    /// Intermediate grid used for rendering. Wrapped in an Option so we can take ownership
    /// temporarily while swapping it with the simulation thread.
    render_grid: Option<Grid>,
}

impl<'app, const PALETTE_RES: usize> App<'app, '_, PALETTE_RES> {
    /// Initialize the App.
    fn new(
        is_running: &'app AtomicBool,
        render_to_sim_tx: mpsc::Sender<Grid>,
        sim_to_render_rx: mpsc::Receiver<Grid>,
        config_tx: mpsc::Sender<Config>,
        config_watcher: ConfigWatcher,
        palette: Palette<PALETTE_RES>,
        render_grid: Grid,
    ) -> Self {
        let window_attributes = WindowAttributes::default()
            .with_title("Trippy Ants (Space: save screenshot, Esc: quit)")
            .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT))
            .with_resizable(false);

        Self {
            config_watcher,
            frame: None,
            window: None,
            window_attributes,
            palette,
            is_running,
            render_to_sim_tx,
            sim_to_render_rx,
            config_tx,
            frame_timeout: Instant::now() + FRAME_TIME,
            render_grid: Some(render_grid),
        }
    }

    /// Update the config if the `config_watcher` found an updated config.
    ///
    /// # Panics
    /// If the simulation thread has exited.
    fn update_config(&mut self) {
        if let Some(new_config) = self.config_watcher.watch_for_update() {
            println!("config updated");
            if let Ok(config_str) = ser::to_string(&new_config) {
                println!("loaded config:\n{config_str}");
            }

            self.palette = Palette::<PALETTE_RES>::new(&new_config.colors);

            // Forward the updated config ownership to the simulation thread
            self.config_tx
                .send(new_config)
                .expect("Simulation thread refused to accept the config");
        }
    }

    /// Helper to cleanly exit.
    fn exit(&self, event_loop: &ActiveEventLoop) {
        self.is_running.store(false, AtomicOrdering::Relaxed);
        event_loop.exit();
    }
}

#[expect(
    clippy::missing_trait_methods,
    reason = "We want to ignore all other events"
)]
impl<const PALETTE_RES: usize> ApplicationHandler for App<'_, '_, PALETTE_RES> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let win = Arc::from(
            event_loop
                .create_window(self.window_attributes.clone())
                .expect("Failed to create window"),
        );
        let window_size = win.inner_size();

        eprintln!(
            "Initializing {}x{} Window",
            window_size.width, window_size.height
        );

        let surface_texture =
            SurfaceTexture::new(window_size.width, window_size.height, Arc::clone(&win));
        let pixels = Pixels::new(u32::from(WIDTH), u32::from(HEIGHT), surface_texture)
            .expect("Failed to initialize Pixels buffer");

        self.frame = Some(Frame::new(WIDTH, HEIGHT, pixels));
        self.window = Some(win);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if Instant::now() >= self.frame_timeout
            && let Some(window) = self.window.as_ref()
        {
            window.request_redraw();
        }
        // This instructs the winit event loop to sleep the thread until the exact timeout.
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.frame_timeout));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        #[expect(
            clippy::pattern_type_mismatch,
            reason = "We want a mutable refernce to each field"
        )]
        let Self {
            frame,
            window,
            palette,
            render_to_sim_tx,
            sim_to_render_rx,
            frame_timeout,
            render_grid,
            ..
        } = self;

        let Some(frame) = frame.as_mut() else {
            return;
        };

        let Some(_window) = window.as_ref() else {
            return;
        };

        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "We want to ignore all other events"
        )]
        match event {
            WindowEvent::CloseRequested => self.exit(event_loop),
            WindowEvent::KeyboardInput {
                event: input_event, ..
            } if input_event.state.is_pressed() => match input_event.physical_key {
                PhysicalKey::Code(KeyCode::Escape) => self.exit(event_loop),
                PhysicalKey::Code(KeyCode::Space) => {
                    let filename = format!(
                        "trippy-ants_{}.png",
                        Local::now().format("%Y-%m-%d_%H-%M-%S")
                    );
                    match frame.save_png(Path::new(&filename)) {
                        Ok(()) => println!("saved {filename}"),
                        Err(error) => eprintln!("failed to save {filename}: {error}"),
                    }
                }
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                // Request a frame update by sending our buffer to the simulation thread.
                if let Some(grid) = render_grid.take()
                    && render_to_sim_tx.send(grid).is_ok()
                    && let Ok(new_grid) = sim_to_render_rx.recv()
                {
                    *render_grid = Some(new_grid);
                }

                if let Some(grid) = render_grid.as_ref() {
                    frame.update(grid, palette);
                } else {
                    // We lost the grid because the channel disconnected (sim thread died).
                    self.exit(event_loop);
                    return;
                }

                if let Err(err) = frame.render() {
                    eprintln!("pixels.render() failed: {err}");
                    self.exit(event_loop);
                    return;
                }

                // Advance the timeout. A loop prevents queueing instant back-to-back renders
                // if the application lagged momentarily.
                let now = Instant::now();
                while *frame_timeout <= now {
                    *frame_timeout += FRAME_TIME;
                }

                self.update_config();
            }
            _ => {}
        }
    }
}

fn main() -> ExitCode {
    let num_sim_threads = env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or_else(|| {
            thread::available_parallelism()
                .map_or(1, NonZero::get)
                .min(8)
        });

    let mut config_watcher = ConfigWatcher::new();

    let config = if let Some(config_file) = env::args().nth(1) {
        match config_watcher.load_config(config_file) {
            Ok(cfg) => cfg,
            Err(err) => {
                eprintln!("{err}");
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

    let palette = Palette::<1024>::new(&config.colors);
    let simulation = Simulation::new(WIDTH, HEIGHT, &config.world);
    let render_grid = simulation.make_scratch_grid();
    let agents = (0..config.agent.count)
        .map(|index| Agent::new(&config, WIDTH, HEIGHT, index))
        .collect::<Vec<_>>();

    let is_running = AtomicBool::new(true);

    let (render_to_sim_tx, render_to_sim_rx) = mpsc::channel::<Grid>();
    let (sim_to_render_tx, sim_to_render_rx) = mpsc::channel::<Grid>();
    let (config_tx, config_rx) = mpsc::channel::<Config>();

    let Ok(event_loop) = EventLoop::new() else {
        eprintln!("Failed to initialize window event loop");
        return ExitCode::FAILURE;
    };

    thread::scope(|scope| {
        #[expect(
            clippy::shadow_same,
            reason = "Explicitly capturing reference for thread scope"
        )]
        let is_running = &is_running;

        let simulation_thread = thread::Builder::new()
            .name("sim_worker_0".to_owned())
            .spawn_scoped(scope, move || {
                rayon::ThreadPoolBuilder::new()
                    .thread_name(|i| format!("sim_worker_{i}"))
                    .num_threads(num_sim_threads)
                    .use_current_thread()
                    .build_global()
                    .expect("Failed to build rayon threadpool");

                Simulator::new(
                    simulation,
                    agents,
                    is_running,
                    render_to_sim_rx,
                    sim_to_render_tx,
                    config_rx,
                )
                .run();
            });

        let mut app = App::<1024>::new(
            is_running,
            render_to_sim_tx,
            sim_to_render_rx,
            config_tx,
            config_watcher,
            palette,
            render_grid,
        );

        let app_error = event_loop.run_app(&mut app).is_err();

        is_running.store(false, AtomicOrdering::Relaxed);

        let simulation_error = simulation_thread.map_or(true, |handle| handle.join().is_err());

        if app_error || simulation_error {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    })
}
