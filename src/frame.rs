//! The part of the simulation which handles the display and screenshot saving of the simulation.

use std::path::Path;

use image::error::{LimitError, LimitErrorKind};
use minifb::Window;
use rayon::{
    iter::{IndexedParallelIterator as _, ParallelIterator as _},
    slice::ParallelSliceMut as _,
};

use crate::{grid::Grid, palette::Palette};

/// Holds the current colorized image representation of the simulation.
pub(crate) struct Frame {
    /// Width of the frame in pixels.
    width: usize,

    /// Height of the frame in pixels.
    height: usize,

    /// minifb: 0x00RRGGBB per pixel, row-major.
    pub(crate) pixels: Vec<u32>,
}

impl Frame {
    /// Create a new frame with the given width and height in pixels.
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            width: usize::from(width),
            height: usize::from(height),
            pixels: vec![0; usize::from(width) * usize::from(height)],
        }
    }

    /// Update the frame with the current state of the simulation by colorizing the stored cell-values.
    ///
    /// # Panics
    ///
    /// Panics if the frame and grid sizes do not match.
    pub(crate) fn update<const RESOLUTION: usize>(
        &mut self,
        grid: &Grid,
        palette: &Palette<RESOLUTION>,
    ) {
        self.pixels
            .par_chunks_exact_mut(self.width)
            .enumerate()
            .for_each(|(y, pixels)| {
                let row = grid.row(y).expect("mismatch between frame and grid size");
                for (pixel, cell) in pixels.iter_mut().zip(row) {
                    *pixel = palette.get_color(cell.level);
                }
            });
    }

    /// Update the window with the current state of the frame.
    ///
    /// # Panics
    ///
    /// Panics if the window's size and the frame's size do not match.
    pub(crate) fn update_window(&self, window: &mut Window) {
        window
            .update_with_buffer(&self.pixels, self.width, self.height)
            .expect("update");
    }

    /// store the current image as a PNG file.
    ///
    /// # Errors
    ///
    /// Returns an error if the PNG file cannot be saved.
    pub(crate) fn save_png(&self, path: &Path) -> Result<(), image::ImageError> {
        let rgb = self
            .pixels
            .iter()
            .flat_map(|rgb| {
                let [blue, green, red, _] = rgb.to_le_bytes();
                [red, green, blue]
            })
            .collect::<Vec<_>>();
        let dimension_error = |_error| {
            image::ImageError::Limits(LimitError::from_kind(LimitErrorKind::DimensionError))
        };
        image::save_buffer(
            path,
            &rgb,
            self.width.try_into().map_err(dimension_error)?,
            self.height.try_into().map_err(dimension_error)?,
            image::ColorType::Rgb8,
        )
    }
}
