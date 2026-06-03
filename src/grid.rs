//! The _backplane_ of the simulation which stores the pheromone levels for each cell in the grid.

use std::{mem, slice, sync::atomic::AtomicU32};

use crate::config::GridTopology;

/// Contains the data for a single cell (pixel) in the grid.
#[derive(Default, Clone, Copy)]
#[repr(transparent)]
pub(crate) struct Cell {
    /// The pheromone level of the cell.
    ///
    /// This level is typically between -1.0 and 1.0. Positive values will attract ants, negative
    /// values will repel them.
    pub(crate) level: f32,
}

#[repr(C, align(64))]
#[derive(Clone, Copy)]
/// Cache Line aligned block of cells.
struct CellBlock([Cell; 16]);

impl CellBlock {
    /// Number of cells in each `CellBlock`.
    const NUM_CELLS: usize = size_of::<Self>() / size_of::<Cell>();
}

/// Data structure holding the cell values for each pixel in the grid.
pub(crate) struct Grid {
    /// Width of the grid in cells/pixels.
    width: u16,

    /// Height of the grid in cells/pixels.
    height: u16,

    /// The actual cells/pixels in the grid.
    blocks: Vec<CellBlock>,

    /// The topology of the grid.
    ///
    /// This determines how the edges of the grid are handled.
    pub(crate) topology: GridTopology,
}

impl Grid {
    /// Create a new grid with the given width and height in cells/pixels.
    ///
    /// # Panics
    ///
    /// Panics if the width or height is 0 or greater than 32767.
    pub(crate) fn new(width: u16, height: u16, topology: GridTopology) -> Self {
        assert!(width > 2, "width must be greater than 2");
        assert!(height > 2, "height must be greater than 2");
        assert!(
            i16::try_from(width).is_ok(),
            "width must be less than or equal to {}",
            i16::MAX
        );
        assert!(
            i16::try_from(height).is_ok(),
            "height must be less than or equal to {}",
            i16::MAX
        );

        let total_cells = usize::from(width) * usize::from(height);
        let num_blocks = total_cells / CellBlock::NUM_CELLS;

        let blocks = vec![CellBlock([Cell::default(); CellBlock::NUM_CELLS]); num_blocks];

        Self {
            width,
            height,
            blocks,
            topology,
        }
    }

    /// Get a read only view to all cells in the grid.
    #[expect(unsafe_code, reason = "conversion of &CellBlock to &[Cell; 16]")]
    pub(crate) fn cells(&self) -> &[Cell] {
        // Safety: each CellBlock has the same layout as `[Cell; CellBlock::NUM_CELLS]`
        unsafe {
            slice::from_raw_parts(
                self.blocks.as_ptr().cast::<Cell>(),
                usize::from(self.width) * usize::from(self.height),
            )
        }
    }

    /// Get a mutable view to all cells in the grid.
    #[expect(unsafe_code, reason = "conversion of &CellBlock to &[Cell; 16]")]
    pub(crate) fn cells_mut(&mut self) -> &mut [Cell] {
        // Safety: each CellBlock has the same layout as `[Cell; CellBlock::NUM_CELLS]`
        unsafe {
            slice::from_raw_parts_mut(
                self.blocks.as_mut_ptr().cast::<Cell>(),
                usize::from(self.width) * usize::from(self.height),
            )
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
    #[expect(unsafe_code, reason = "conversion of &CellBlock to &[Cell; 16]")]
    pub(crate) fn row(&self, y: impl Into<usize>) -> Option<&[Cell]> {
        let y = y.into();
        if y >= self.height as usize {
            return None;
        }

        // Calculate the slice of blocks that represent this row
        let blocks_per_row = self.width as usize / CellBlock::NUM_CELLS;
        let start = y * blocks_per_row;
        let end = start + blocks_per_row;

        let block_slice = self.blocks.get(start..end)?;
        // Safety: each CellBlock has the same layout as `[Cell; CellBlock::NUM_CELLS]`
        unsafe {
            Some(slice::from_raw_parts(
                block_slice.as_ptr().cast::<Cell>(),
                blocks_per_row * CellBlock::NUM_CELLS,
            ))
        }
    }

    /// Get a mutable row of cells from the grid.
    #[expect(unsafe_code, reason = "conversion of &CellBlock to &[Cell; 16]")]
    pub(crate) fn row_mut(&mut self, y: impl Into<usize>) -> Option<&mut [Cell]> {
        let y = y.into();
        if y >= self.height as usize {
            return None;
        }

        let blocks_per_row = self.width as usize / CellBlock::NUM_CELLS;
        let start = y * blocks_per_row;
        let end = start + blocks_per_row;

        let block_slice = self.blocks.get_mut(start..end)?;
        // Safety: each CellBlock has the same layout as `[Cell; CellBlock::NUM_CELLS]`
        unsafe {
            Some(slice::from_raw_parts_mut(
                block_slice.as_mut_ptr().cast::<Cell>(),
                blocks_per_row * CellBlock::NUM_CELLS,
            ))
        }
    }

    /// Get an iterator over the rows of cells in the grid.
    pub(crate) fn rows_mut(&mut self) -> impl Iterator<Item = &mut [Cell]> {
        let width = self.width;
        self.cells_mut().chunks_exact_mut(usize::from(width))
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
    pub(crate) fn index(&self, x: f32, y: f32) -> usize {
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
        &self.cells()[index]
    }

    /// Returns a reference to the cell's level as an `AtomicU32` (bitcast).
    ///
    /// # Safety
    ///
    /// The caller must ensure that there are no non‑atomic accesses (reads or writes
    /// via `&f32` or `&mut f32`) to the same memory location for the entire duration
    /// any `&AtomicU32` returned by this function is live, and that the returned
    /// reference is not used to introduce a data race.
    /// (See <https://doc.rust-lang.org/1.96.0/std/sync/atomic/index.html> for details on the atomic memory model).
    #[expect(
        unsafe_code,
        reason = "required to access the cells atomically in some cases but non-atomically in others"
    )]
    pub(crate) unsafe fn atomic_cell_level(&self, x: f32, y: f32) -> &AtomicU32 {
        let index = self.index(x, y);

        #[expect(
            clippy::indexing_slicing,
            reason = "The `index` method ensures that the index is in bounds"
        )]
        #[expect(
            clippy::transmute_ptr_to_ptr,
            unsafe_code,
            reason = "We cast a f32 as an AtomicU32"
        )]
        // Safety:
        // - We use transmute to turn a &f32 into a &AtomicU32 which is safe, representation wise.
        // - The caller is responsible for ensuring no concurrent non-atomic access.
        unsafe {
            mem::transmute(&self.cells()[index].level)
        }
    }

    /// Evaluates a 3x3 Gaussian filter kernel given three horizontal row segments.
    ///
    /// filter kernel (weight sum = 16)
    /// 1 2 1
    /// 2 4 2
    /// 1 2 1.
    fn blur_kernel(top: [Cell; 3], mid: [Cell; 3], bot: [Cell; 3]) -> f32 {
        let corners = top[0].level + top[2].level + bot[0].level + bot[2].level;
        let sides = top[1].level + mid[0].level + bot[1].level + mid[2].level;
        let center = mid[1].level;

        // Sum up smallest values first for improved numerical stability
        let halo = corners.mul_add(16.0_f32.recip(), sides * 8.0_f32.recip());
        center.mul_add(4.0_f32.recip(), halo)
    }

    #[inline]
    #[expect(
        clippy::indexing_slicing,
        clippy::missing_asserts_for_indexing,
        reason = "the bounds are checked once beforehand"
    )]
    /// # Panics
    /// - if the grids don't match in dimensions
    /// - if the grid is malformed
    pub(crate) fn blur(&mut self, read_buffer: &Self, decay_factor: f32) {
        let width = self.width as usize;
        let height = self.height as usize;

        // Ensure that the grids match
        assert_eq!(
            self.cells().len(),
            read_buffer.cells().len(),
            "incompatible Grid layout"
        );
        assert_eq!(self.height, read_buffer.height, "incompatible Grid shape");
        assert_eq!(self.width % 64, 0, "incompatible Grid width");

        assert_eq!(self.cells().len(), width * height, "bad grid layout");
        assert!(width >= 3 && height >= 3, "grid is too small");
        assert!(
            i16::try_from(width).is_ok() && i16::try_from(height).is_ok(),
            "bad grid size"
        );

        // Process interior rows
        #[expect(
            clippy::cast_possible_wrap,
            clippy::cast_possible_truncation,
            reason = "checked beforehand"
        )]
        let x_right = read_buffer.map_col(width as i16) as usize;
        let x_left = read_buffer.map_col(-1) as usize;

        rayon::scope(|scope| {
            let mut remaining_cells = &mut self.cells_mut()[width..(height - 1) * width];
            let mut current_y = 1;

            let num_workers = rayon::current_num_threads();
            let rows_per_worker = height.saturating_sub(2) / num_workers;
            let remainder = height.saturating_sub(2) % num_workers;

            for i in 0..num_workers {
                // Distribute any remainder rows across the first few chunks
                let rows_for_this_worker = rows_per_worker + usize::from(i < remainder);

                if rows_for_this_worker == 0 {
                    continue;
                }

                let cells_for_this_worker = rows_for_this_worker * width;
                let (chunk, rest) = remaining_cells.split_at_mut(cells_for_this_worker);
                remaining_cells = rest;

                let start_y = current_y;
                current_y += rows_for_this_worker;

                scope.spawn(move |_| {
                    for (j, write_row) in chunk.chunks_exact_mut(width).enumerate() {
                        let y = start_y + j;
                        let (row_top, row_mid, row_bot) = (|| {
                            Some((
                                read_buffer.row(y - 1)?,
                                read_buffer.row(y)?,
                                read_buffer.row(y + 1)?,
                            ))
                        })()
                        .expect("Failed to get row neighborhood");

                        // Left Boundary Cell
                        write_row[0].level = Self::blur_kernel(
                            [row_top[x_left], row_top[0], row_top[1]],
                            [row_mid[x_left], row_mid[0], row_mid[1]],
                            [row_bot[x_left], row_bot[0], row_bot[1]],
                        ) * decay_factor;

                        let top_wins = row_top[..width].array_windows::<3>();
                        let mid_wins = row_mid[..width].array_windows::<3>();
                        let bot_wins = row_bot[..width].array_windows::<3>();
                        let out_slice = &mut write_row[1..width - 1];

                        for (((dst, top), mid), bot) in out_slice
                            .iter_mut()
                            .zip(top_wins)
                            .zip(mid_wins)
                            .zip(bot_wins)
                        {
                            dst.level = Self::blur_kernel(*top, *mid, *bot) * decay_factor;
                        }

                        // Right Boundary Cell
                        write_row[width - 1].level = Self::blur_kernel(
                            [row_top[width - 2], row_top[width - 1], row_top[x_right]],
                            [row_mid[width - 2], row_mid[width - 1], row_mid[x_right]],
                            [row_bot[width - 2], row_bot[width - 1], row_bot[x_right]],
                        ) * decay_factor;
                    }
                });
            }
        });

        // Process Boundary Rows (y = 0 and y = height - 1)
        for y in [0, height - 1] {
            let y16 = i16::try_from(y).expect("invalid row index");
            let (row_top, row_mid, row_bot) = (|| {
                Some((
                    read_buffer.row(read_buffer.map_row(y16 - 1) as usize)?,
                    read_buffer.row(y)?,
                    read_buffer.row(read_buffer.map_row(y16 + 1) as usize)?,
                ))
            })()
            .expect("Failed to get boundary row neighborhood");

            let write_row = &mut self.cells_mut()[y * width..(y + 1) * width];
            for (x, dst) in write_row.iter_mut().enumerate() {
                let x16 = i16::try_from(x).expect("invalid column index");
                let cols = [
                    read_buffer.map_col(x16 - 1) as usize,
                    x,
                    read_buffer.map_col(x16 + 1) as usize,
                ];

                dst.level = Self::blur_kernel(
                    cols.map(|i| row_top[i]),
                    cols.map(|i| row_mid[i]),
                    cols.map(|i| row_bot[i]),
                ) * decay_factor;
            }
        }
    }
}
