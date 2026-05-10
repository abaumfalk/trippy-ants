use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::ParallelSliceMut,
};

use crate::{Agent, config::GridTopology};

#[derive(Default, Clone, Copy)]
pub(crate) struct Cell {
    pub(crate) level: f32,
}

pub(crate) struct Grid {
    width: usize,
    height: usize,
    pub(crate) cells: Vec<Cell>,
    topology: GridTopology,
}

impl Grid {
    fn new(width: usize, height: usize, topology: GridTopology) -> Self {
        Self {
            width,
            height,
            cells: vec![Cell::default(); width * height],
            topology,
        }
    }

    fn map_row(&self, y: i32) -> usize {
        let height = self.height as i32;
        match self.topology {
            GridTopology::Torus => {
                if (0..height).contains(&y) {
                    y as usize
                } else {
                    y.rem_euclid(height) as usize
                }
            }
            GridTopology::Plane => y.clamp(0, height - 1) as usize,
        }
    }

    fn map_col(&self, x: i32) -> usize {
        let width = self.width as i32;
        match self.topology {
            GridTopology::Torus => {
                if (0..width).contains(&x) {
                    x as usize
                } else {
                    x.rem_euclid(width) as usize
                }
            }
            GridTopology::Plane => x.clamp(0, width - 1) as usize,
        }
    }

    pub(crate) fn row(&self, y: i32) -> &[Cell] {
        &self.cells[self.map_row(y) * self.width..][..self.width]
    }

    fn row_mut(&mut self, y: i32) -> &mut [Cell] {
        let mapped_row = self.map_row(y);
        &mut self.cells[mapped_row * self.width..][..self.width]
    }

    fn index(&self, x: f32, y: f32) -> usize {
        let x = (x.round() as usize).clamp(0, self.width - 1);
        let y = (y.round() as usize).clamp(0, self.height - 1);
        y * self.width + x
    }

    pub(crate) fn cell(&self, x: f32, y: f32) -> &Cell {
        let index = self.index(x, y);
        &self.cells[index]
    }

    fn cell_mut(&mut self, x: f32, y: f32) -> &mut Cell {
        let index = self.index(x, y);
        &mut self.cells[index]
    }
}

pub(crate) struct Simulation {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) read_buffer: Grid,
    pub(crate) write_buffer: Grid,
    pub(crate) limit: f32,
    pub(crate) enable_walls: bool,
}

impl Simulation {
    pub(crate) fn new(
        width: usize,
        height: usize,
        limit: f32,
        enable_walls: bool,
        topology: GridTopology,
    ) -> Self {
        Self {
            width,
            height,
            read_buffer: Grid::new(width, height, topology),
            write_buffer: Grid::new(width, height, topology),
            limit,
            enable_walls,
        }
    }

    pub(crate) fn blur(&mut self, decay_factor: f32, grid_topology: GridTopology) {
        let max_y = self.height - 1;
        let max_x = self.width - 1;

        self.write_buffer
            .cells
            .par_chunks_exact_mut(self.width)
            .enumerate()
            .for_each(|(y, write_row)| {
                // 5 rows around the current row
                let y = y as i32;
                let row = [
                    self.read_buffer.row(y - 2),
                    self.read_buffer.row(y - 1),
                    self.read_buffer.row(y),
                    self.read_buffer.row(y + 1),
                    self.read_buffer.row(y + 2),
                ];
                for (x, write_cell) in write_row.iter_mut().enumerate() {
                    // column indices for the 5 columns around x
                    let x = x as i32;
                    let col = [
                        self.read_buffer.map_col(x - 2),
                        self.read_buffer.map_col(x - 1),
                        self.read_buffer.map_col(x),
                        self.read_buffer.map_col(x + 1),
                        self.read_buffer.map_col(x + 2),
                    ];

                    let mut sum = 0.0;
                    // sum += row[0][col[1]].level;
                    // sum += row[0][col[2]].level;
                    // sum += row[0][col[3]].level;

                    // sum += row[1][col[0]].level;
                    sum += row[1][col[1]].level * 1.0;
                    sum += row[1][col[2]].level * 2.0;
                    sum += row[1][col[3]].level * 1.0;
                    // sum += row[1][col[4]].level;

                    // sum += row[2][col[0]].level;
                    sum += row[2][col[1]].level * 2.0;
                    sum += row[2][col[2]].level * 4.0;
                    sum += row[2][col[3]].level * 2.0;
                    // sum += row[2][col[4]].level;

                    // sum += row[3][col[0]].level;
                    sum += row[3][col[1]].level * 1.0;
                    sum += row[3][col[2]].level * 2.0;
                    sum += row[3][col[3]].level * 1.0;
                    // sum += row[3][col[4]].level;

                    // sum += row[4][col[1]].level;
                    // sum += row[4][col[2]].level;
                    // sum += row[4][col[3]].level;

                    let level = (sum / 16.0) * decay_factor;

                    write_cell.level = if level.is_normal() { level } else { 0.0 };
                }
            });
    }

    pub(crate) fn update(&mut self, agents: &mut [Agent]) {
        for agent in agents.iter() {
            let limit = self.limit;
            let level = &mut self.write_buffer.cell_mut(agent.x, agent.y).level;
            *level = (*level + agent.value).min(limit);
        }

        // for angle in 0..1000 {
        //     if (angle / 125) % 2 == 0 {
        //         continue;
        //     }
        //     let a = angle as f32 / 1000.0;
        //     let angle = a * TAU;
        //     let (sin, cos) = angle.sin_cos();
        //     let r = self.height as f32 * 0.25; // - angle * 10.0;
        //     let level = &mut self
        //         .cell_mut(
        //             self.width as f32 * 0.5 + cos * r,
        //             self.height as f32 * 0.5 + sin * r,
        //         )
        //         .level;
        //     *level = 1.0; //level.max(1.0 - a);
        // }

        // let mut draw_line = |x1: f32, y1: f32, x2: f32, y2: f32| {
        //     let (dx, dy) = (x2 - x1, y2 - y1);
        //     let steps = (dx.abs().max(dy.abs()) as usize).max(1);
        //     for i in 0..steps {
        //         let x = x1 + dx * i as f32 / steps as f32;
        //         let y = y1 + dy * i as f32 / steps as f32;
        //         self.cell_mut(x, y).level = 1.0;
        //     }
        // };

        // let w = WIDTH as f32;
        // let h = HEIGHT as f32;

        // draw_line(w * 0.5, h * 0.2, w * 0.7, h * 0.8);
        // draw_line(w * 0.5, h * 0.2, w * 0.3, h * 0.8);
        // draw_line(w * 0.3, h * 0.8, w * 0.7, h * 0.8);

        // repulse from walls
        if self.enable_walls {
            let value = 0.0;
            self.write_buffer.row_mut(0).iter_mut().for_each(|cell| {
                cell.level = value;
            });
            self.write_buffer
                .row_mut(self.height as i32 - 1)
                .iter_mut()
                .for_each(|cell| {
                    cell.level = value;
                });
            for y in 0..self.height {
                let row_index = y * self.width;
                self.write_buffer.cells[row_index].level = value;
                self.write_buffer.cells[row_index + self.width - 1].level = value;
            }
        }

        // let row = (self.height / 2 - 1) * self.width;
        // for x in 0..self.width {
        //     // Hot coals + noise along the base.
        //     let bump = rand_u32(rng) % 96;
        //     self.cells[row + x].level = (220.0 - bump as f32) / 255.0;
        // }

        std::mem::swap(&mut self.read_buffer, &mut self.write_buffer);
    }
}
