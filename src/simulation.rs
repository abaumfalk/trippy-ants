//! The current run-time state of the entire simulation.

use std::{mem, sync::atomic::Ordering};

use crate::{agent::Agent, config::WorldConfig, grid::Grid};

/// The current run-time state of the entire simulation.
pub(crate) struct Simulation {
    /// Width of the grid in cells/pixels.
    pub(crate) width: u16,

    /// Height of the grid in cells/pixels.
    pub(crate) height: u16,

    /// The buffer that is read from in the current frame.
    ///
    /// This will be swapped with the write buffer after each frame.
    pub(crate) read_buffer: Grid,

    /// The buffer that is written to in the current frame.
    ///
    /// This will be swapped with the read buffer after each frame.
    pub(crate) write_buffer: Grid,

    /// The value to use for the outermost pixel rows and columns.
    ///
    /// This will repel or attract the ants from the edges of the grid.
    /// A value of `None` means that the edges have no special effect.
    pub(crate) wall_value: Option<f32>,

    /// The decay factor to use for the pheromone levels.
    pub(crate) decay_factor: f32,
}

impl Simulation {
    /// Create a new simulation with the given width and height in cells/pixels.
    pub(crate) fn new(width: u16, height: u16, config: &WorldConfig) -> Self {
        let WorldConfig {
            wall_value,
            topology,
            decay_factor,
        } = *config;

        Self {
            width,
            height,
            read_buffer: Grid::new(width, height, topology),
            write_buffer: Grid::new(width, height, topology),
            wall_value,
            decay_factor,
        }
    }

    /// Allocate a scratch grid matching the simulation.
    pub(crate) fn make_scratch_grid(&self) -> Grid {
        Grid::new(self.width, self.height, self.read_buffer.topology)
    }

    /// Update the simulation by blurring the pheromone levels of the read buffer and writing them to the write buffer.
    pub(crate) fn blur(&mut self) {
        self.write_buffer.blur(&self.read_buffer, self.decay_factor);
    }

    /// Update the simulation by adding the pheromone levels of the agents to the write buffer.
    pub(crate) fn apply_agents(&self, agents: &[Agent]) {
        let total_agents = agents.len();

        if total_agents == 0 {
            return;
        }

        rayon::scope(|scope| {
            let mut remaining_agents = agents;

            let num_workers = rayon::current_num_threads();
            let agents_per_worker = total_agents / num_workers;
            let remainder = total_agents % num_workers;

            for i in 0..num_workers {
                // Distribute any remainder agents across the first few chunks
                let agents_for_this_worker = agents_per_worker + usize::from(i < remainder);

                if agents_for_this_worker == 0 {
                    continue;
                }

                // Safely split the immutable slice
                let (chunk, rest) = remaining_agents.split_at(agents_for_this_worker);
                remaining_agents = rest;

                scope.spawn(move |_| {
                    for agent in chunk {
                        #[expect(unsafe_code, reason = "required to access the cells atomically in some cases but non-atomically in others")]
                        // Safety: All atomic work on the write buffer happens within this
                        // `rayon::scope()`, which blocks until spawned tasks complete, acting as a
                        // synchronization point. Inside the scope, we only perform atomic operations
                        // on the cell levels. After `scope()` returns, no concurrent accesses remain,
                        // so subsequent non-atomic access is safe.
                        let atomic_level =
                            unsafe { self.write_buffer.atomic_cell_level(agent.x, agent.y) };

                        let mut level = atomic_level.load(Ordering::Relaxed);

                        loop {
                            let new_level = (f32::from_bits(level) + agent.value)
                                .clamp(-1.0, 1.0)
                                .to_bits();

                            match atomic_level.compare_exchange_weak(
                                level,
                                new_level,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => break,
                                Err(old_level) => level = old_level,
                            }
                        }
                    }
                });
            }
        });
    }

    /// Apply the wall value to the outermost pixel rows and columns.
    pub(crate) fn apply_bc(&mut self) {
        // repulse from or attract to walls
        if let Some(value) = self.wall_value {
            // top wall
            self.write_buffer
                .first_row_mut()
                .iter_mut()
                .for_each(|cell| {
                    cell.level = value;
                });

            // bottom wall
            self.write_buffer
                .last_row_mut()
                .iter_mut()
                .for_each(|cell| {
                    cell.level = value;
                });

            // sides
            for row in self.write_buffer.rows_mut() {
                if let Some(first) = row.first_mut() {
                    // left wall
                    first.level = value;
                }
                if let Some(last) = row.last_mut() {
                    // right wall
                    last.level = value;
                }
            }
        }

        // let row = (self.height / 2 - 1) * self.width;
        // for x in 0..self.width {
        //     // Hot coals + noise along the base.
        //     let bump = rand_u32(rng) % 96;
        //     self.cells[row + x].level = (220.0 - bump as f32) / 255.0;
        // }
    }

    /// Swap the read and write buffers.
    ///
    /// This will be called after each frame to prepare for the next frame.
    pub(crate) const fn swap_buffers(&mut self) {
        mem::swap(&mut self.read_buffer, &mut self.write_buffer);
    }

    /// Update the simulation with the new configuration.
    pub(crate) const fn update_config(&mut self, new_config: &WorldConfig) {
        let WorldConfig {
            wall_value,
            topology,
            decay_factor,
        } = *new_config;
        self.wall_value = wall_value;
        self.decay_factor = decay_factor;
        self.read_buffer.topology = topology;
        self.write_buffer.topology = topology;
    }
}
