//! Color mapping for the pheromone levels.

use crate::config::ColorConfig;

/// Lookup table for the colorization of the pheromone levels.
///
/// The resolution of the lookup table determines the number of colors in the palette. Higher values
/// will result in a smoother gradient but might impact performance.
/// Values between 256 and 4096 are recommended.
pub(crate) struct Palette<const RESOLUTION: usize> {
    /// Lookup table for the colorization of the normal (positive) pheromone levels.
    colors: [u32; RESOLUTION],

    /// Lookup table for the colorization of the anti-ants (negative) pheromone levels.
    anti_colors: [u32; RESOLUTION],
}

impl<const RESOLUTION: usize> Palette<RESOLUTION> {
    /// Create a new palette with the given color configuration.
    ///
    /// # Panics
    ///
    /// Panics at compile time if the resolution is 0.
    pub(crate) fn new(config: &ColorConfig) -> Self {
        assert!(RESOLUTION > 0, "Palette resolution must be greater than 1");

        let colors = Self::build_palette(config.normal);
        let anti_colors = Self::build_palette(config.anti);
        Self {
            colors,
            anti_colors,
        }
    }

    /// Get the color for the given pheromone level.
    #[expect(clippy::missing_panics_doc, reason = "unreachable")]
    pub(crate) fn get_color(&self, level: f32) -> u32 {
        if level >= 0.0 {
            #[expect(clippy::cast_possible_truncation, reason = "truncation is intentional")]
            #[expect(clippy::cast_sign_loss, reason = "sign has been checked")]
            #[expect(clippy::cast_precision_loss, reason = "quantization is intentional")]
            let index = (level * self.colors.len() as f32) as usize;
            #[expect(
                clippy::unwrap_used,
                reason = "`colors` is guaranteed to have at least one element"
            )]
            let clipping_color = || self.colors.last().copied().unwrap();
            self.colors
                .get(index)
                .copied()
                .unwrap_or_else(clipping_color)
        } else {
            #[expect(clippy::cast_possible_truncation, reason = "truncation is intentional")]
            #[expect(clippy::cast_sign_loss, reason = "sign has been checked")]
            #[expect(clippy::cast_precision_loss, reason = "quantization is intentional")]
            let index = (-level * self.anti_colors.len() as f32) as usize;
            #[expect(
                clippy::unwrap_used,
                reason = "`anti_colors` is guaranteed to have at least one element"
            )]
            let clipping_color = || self.anti_colors.last().copied().unwrap();
            self.anti_colors
                .get(index)
                .copied()
                .unwrap_or_else(clipping_color)
        }
    }

    /// Build a lookup table for the colorization of the pheromone levels using the given color weights.
    fn build_palette([r_exp, g_exp, b_exp]: [f32; 3]) -> [u32; RESOLUTION] {
        let mut result = [0; RESOLUTION];
        #[expect(clippy::cast_precision_loss, reason = "length is small enough")]
        let index_scale = ((result.len() - 1) as f32).recip();
        for (index, color) in result.iter_mut().enumerate() {
            // map index to 0.0..=1.0
            #[expect(clippy::cast_precision_loss, reason = "index is small enough")]
            let level = index as f32 * index_scale;

            // color curve bending; an exponent of 0.0 will result in a linear mapping.
            let red = level.powf((-r_exp).exp2());
            let green = level.powf((-g_exp).exp2());
            let blue = level.powf((-b_exp).exp2());

            // map and clamp colors to 0..=255
            #[expect(
                clippy::cast_possible_truncation,
                reason = "clamped value will be in range"
            )]
            #[expect(clippy::cast_sign_loss, reason = "clamped value cannot be negative")]
            let map_range = |value: f32| (value * 256.0).round_ties_even().clamp(0.0, 255.0) as u8;
            let red = map_range(red);
            let green = map_range(green);
            let blue = map_range(blue);
            let alpha = 0xff;

            // combine colors into a single u32 (0xAABBGGRR)
            *color = u32::from_le_bytes([red, green, blue, alpha]);
        }
        result
    }
}
