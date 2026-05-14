//! The _backplane_ of the simulation which stores the pheromone levels for each cell in the grid.

use rayon::{
    iter::{IndexedParallelIterator as _, ParallelIterator as _},
    slice::ParallelSliceMut as _,
};

use crate::config::GridTopology;

/// Contains the data for a single cell (pixel) in the grid.
#[derive(Default, Clone, Copy)]
pub(crate) struct Cell {
    /// The pheromone level of the cell.
    ///
    /// This level is typically between -1.0 and 1.0. Positive values will attract ants, negative
    /// values will repel them.
    pub(crate) level: f32,
}

/// Data structure holding the cell values for each pixel in the grid.
pub(crate) struct Grid {
    /// Width of the grid in cells/pixels.
    width: u16,

    /// Height of the grid in cells/pixels.
    height: u16,

    /// The actual cells/pixels in the grid.
    pub(crate) cells: Vec<Cell>,

    /// The topology of the grid.
    ///
    /// This determines how the edges of the grid are handled.
    topology: GridTopology,
}

impl Grid {
    /// Create a new grid with the given width and height in cells/pixels.
    ///
    /// # Panics
    ///
    /// Panics if the width or height is 0 or greater than 32767.
    pub(crate) fn new(width: u16, height: u16, topology: GridTopology) -> Self {
        assert!(width > 0, "width must be greater than 0");
        assert!(height > 0, "height must be greater than 0");
        assert!(width <= 0x7FFF, "width must be less than or equal to 32767");
        assert!(
            height <= 0x7FFF,
            "height must be less than or equal to 32767"
        );

        Self {
            width,
            height,
            cells: vec![Cell::default(); usize::from(width) * usize::from(height)],
            topology,
        }
    }

    /// Map a row index to the actual row index in the grid.
    ///
    /// The topology will determine whether an out-of-bounds index shall be wrapped around or
    /// clamped to the edge.
    fn map_row(&self, y: i16) -> u16 {
        let height = self.height.cast_signed();
        let y = match self.topology {
            GridTopology::Torus => {
                if (0..height).contains(&y) {
                    y
                } else {
                    y.rem_euclid(height)
                }
            }
            GridTopology::Plane => y.clamp(0, height - 1),
        };
        y.cast_unsigned()
    }

    /// Map a column index to the actual column index in the grid.
    ///
    /// The topology will determine whether an out-of-bounds index shall be wrapped around or
    /// clamped to the edge.
    fn map_col(&self, x: i16) -> u16 {
        let width = self.width.cast_signed();
        let x = match self.topology {
            GridTopology::Torus => {
                if (0..width).contains(&x) {
                    x
                } else {
                    x.rem_euclid(width)
                }
            }
            GridTopology::Plane => x.clamp(0, width - 1),
        };
        x.cast_unsigned()
    }

    /// Get a row of cells from the grid.
    pub(crate) fn row(&self, y: impl Into<usize>) -> Option<&[Cell]> {
        let width = usize::from(self.width);
        let offset = y.into() * width;
        self.cells.get(offset..offset + width)
    }

    /// Get a mutable row of cells from the grid.
    pub(crate) fn row_mut(&mut self, y: impl Into<usize>) -> Option<&mut [Cell]> {
        let width = usize::from(self.width);
        let offset = y.into() * width;
        self.cells.get_mut(offset..offset + width)
    }

    /// Get an iterator over the rows of cells in the grid.
    pub(crate) fn rows_mut(&mut self) -> impl Iterator<Item = &mut [Cell]> {
        self.cells.chunks_exact_mut(usize::from(self.width))
    }

    /// Get the first row of cells in the grid as mutable.
    #[expect(
        clippy::missing_panics_doc,
        reason = "unreachable due to constructor validation"
    )]
    pub(crate) fn first_row_mut(&mut self) -> &mut [Cell] {
        self.row_mut(0_usize)
            .expect("unreachable: no first row exists")
    }

    /// Get the last row of cells in the grid as mutable.
    #[expect(
        clippy::missing_panics_doc,
        reason = "unreachable due to constructor validation"
    )]
    pub(crate) fn last_row_mut(&mut self) -> &mut [Cell] {
        self.row_mut(self.height - 1)
            .expect("unreachable: no last row exists")
    }

    /// Get the cell index for the given x and y coordinates.
    ///
    /// Out-of-bounds indices will be handled according to the topology.
    ///
    /// The returned index is guaranteed to be within the bounds of the grid.
    fn index(&self, x: f32, y: f32) -> usize {
        #[expect(clippy::cast_possible_truncation, reason = "truncation is acceptable")]
        let (x16, y16) = (x.round() as i16, y.round() as i16);
        let (mapped_x, mapped_y) = (self.map_col(x16), self.map_row(y16));
        usize::from(mapped_y) * usize::from(self.width) + usize::from(mapped_x)
    }

    /// Get the cell at the given x and y coordinates.
    ///
    /// Out-of-bounds indices will be handled according to the topology.
    // TODO implement interpolation
    pub(crate) fn cell(&self, x: f32, y: f32) -> &Cell {
        let index = self.index(x, y);
        #[expect(
            clippy::indexing_slicing,
            reason = "The `index` method ensures that the index is in bounds"
        )]
        &self.cells[index]
    }

    /// Get the mutable cell at the given x and y coordinates.
    ///
    /// Out-of-bounds indices will be handled according to the topology.
    // TODO implement interpolation
    pub(crate) fn cell_mut(&mut self, x: f32, y: f32) -> &mut Cell {
        let index = self.index(x, y);
        #[expect(
            clippy::indexing_slicing,
            reason = "The `index` method ensures that the index is in bounds"
        )]
        &mut self.cells[index]
    }

    /// Update the grid by blurring the pheromone levels of the read buffer.
    ///
    /// The decay factor will determine how much the pheromone levels will be reduced.
    #[expect(
        clippy::unwrap_used,
        clippy::missing_panics_doc,
        reason = "unreachable: coordinates are guaranteed to be in bounds"
    )]
    pub(crate) fn blur(&mut self, read_buffer: &Self, decay_factor: f32) {
        self.cells
            .par_chunks_exact_mut(usize::from(self.width))
            .enumerate()
            .for_each(|(y, write_row)| {
                // 3 rows around the current row
                let y16 = i16::try_from(y).unwrap();
                let row = [
                    read_buffer.row(read_buffer.map_row(y16 - 1)).unwrap(),
                    read_buffer.row(y).unwrap(),
                    read_buffer.row(read_buffer.map_row(y16 + 1)).unwrap(),
                ];
                for (x, write_cell) in write_row.iter_mut().enumerate() {
                    // column indices for the 3 columns around x
                    let x16 = i16::try_from(x).unwrap();
                    let col = [
                        usize::from(read_buffer.map_col(x16 - 1)),
                        x,
                        usize::from(read_buffer.map_col(x16 + 1)),
                    ];

                    #[expect(
                        clippy::indexing_slicing,
                        reason = "all indices are either compile-time constants or are guaranteed to be in bounds by using the mapping methods"
                    )]
                    let cell = |x_index: usize, y_index: usize| row[y_index][col[x_index]].level;

                    // filter kernel (weight sum = 16)
                    // 1 2 1
                    // 2 4 2
                    // 1 2 1

                    let value00 = cell(0, 0); // top left
                    let value01 = cell(1, 0); // top center
                    let value02 = cell(2, 0); // top right
                    let value10 = cell(0, 1); // left center
                    let value11 = cell(1, 1); // center
                    let value12 = cell(2, 1); // right center
                    let value20 = cell(0, 2); // bottom left
                    let value21 = cell(1, 2); // bottom center
                    let value22 = cell(2, 2); // bottom right

                    // sum up smallest values first for improved numerical stability
                    let corners = (value00 + value02 + value20 + value22) * 16.0_f32.recip();
                    let sides = (value01 + value10 + value21 + value12) * 8.0_f32.recip();
                    let center = value11 * 4.0_f32.recip();
                    let sum = corners + sides + center;
                    let level = sum * decay_factor;

                    // avoid sub-normal numbers for performance reasons
                    write_cell.level = if level.is_normal() { level } else { 0.0 };
                }
            });
    }
}
