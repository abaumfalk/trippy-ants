//! The part of the simulation which handles the display and screenshot saving of the simulation.

use std::path::Path;

use image::error::{LimitError, LimitErrorKind};
use pixels::Pixels;

use crate::{grid::Grid, palette::Palette};

/// Holds the current colorized image representation of the simulation.
pub(crate) struct Frame<'pixels> {
    /// Width of the frame in pixels.
    width: usize,

    /// Height of the frame in pixels.
    height: usize,

    /// The Pixels backend.
    pixels: Pixels<'pixels>,
}

impl<'pixels> Frame<'pixels> {
    /// Create a new frame with the given width and height in pixels.
    ///
    /// # Panics
    /// If the pixels instance does not match the specified with and height.
    pub(crate) fn new(width: u16, height: u16, pixels: Pixels<'pixels>) -> Self {
        assert_eq!(
            width as usize * height as usize * 4,
            pixels.frame().len(),
            "width and height do not match the pixels instance"
        );

        Self {
            width: usize::from(width),
            height: usize::from(height),
            pixels,
        }
    }

    /// Update the frame with the current state of the simulation by colorizing the stored cell-values
    /// and render it to the underlying surface.
    ///
    /// # Panics
    /// If the frame and grid sizes do not match.
    pub(crate) fn update<const RESOLUTION: usize>(
        &mut self,
        grid: &Grid,
        palette: &Palette<RESOLUTION>,
    ) {
        self.pixels
            .frame_mut()
            .chunks_exact_mut(4 * self.width)
            .enumerate()
            .for_each(|(i, pixels)| {
                for (j, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let level = grid
                        .cells()
                        .get(i * self.width + j)
                        .expect("the size of screen_bytes does not match the grid")
                        .level;

                    let color = palette.get_color(level);

                    // The palette is packed as 0xAABBGGRR:
                    // Copy directly to the destination slice in the order expected by pixels: [R, G, B, A]
                    *pixel = color.to_le_bytes();
                }
            });
    }

    /// Render the frame contents to the screen.
    ///
    /// # Errors
    /// If rendering the frame to the underlying surface fails.
    pub(crate) fn render(&self) -> Result<(), pixels::Error> {
        self.pixels.render()
    }

    /// store the current image as a PNG file.
    ///
    /// # Errors
    ///
    /// Returns an error if the PNG file cannot be saved.
    pub(crate) fn save_png(&self, path: &Path) -> Result<(), image::ImageError> {
        let rgba_frame = self.pixels.frame();

        // Directly pull R, G, B channels from the ordered layout
        let rgb = rgba_frame
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[red, green, blue, _alpha]| [red, green, blue])
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
