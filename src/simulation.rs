//! The current run-time state of the entire simulation.

use std::mem;

use crate::{
    agent::Agent,
    config::{Config, WorldConfig},
    grid::Grid,
};

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
    pub(crate) fn new(width: u16, height: u16, config: &Config) -> Self {
        let WorldConfig {
            wall_value,
            topology,
            decay_factor,
        } = config.world;

        Self {
            width,
            height,
            read_buffer: Grid::new(width, height, topology),
            write_buffer: Grid::new(width, height, topology),
            wall_value,
            decay_factor,
        }
    }

    /// Update the simulation by blurring the pheromone levels of the read buffer and writing them to the write buffer.
    pub(crate) fn blur(&mut self) {
        self.write_buffer.blur(&self.read_buffer, self.decay_factor);
    }

    /// Update the simulation by adding the pheromone levels of the agents to the write buffer.
    ///
    /// This will also apply the wall value to the outermost pixel rows and columns.
    pub(crate) fn update(&mut self, agents: &[Agent]) {
        for agent in agents {
            let level = &mut self.write_buffer.cell_mut(agent.x, agent.y).level;
            *level = (*level + agent.value).clamp(-1.0, 1.0);
        }

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
}
