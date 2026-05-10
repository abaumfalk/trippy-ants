use rayon::{
    iter::{IndexedParallelIterator, ParallelBridge, ParallelIterator},
    slice::ParallelSliceMut,
};

use crate::grid::{Cell, Grid, Simulation};

pub(crate) struct Frame {
    width: usize,
    _height: usize,
    pub(crate) pixels: Vec<u32>,
    palette: Palette,
}

impl Frame {
    pub(crate) fn new(width: usize, height: usize, palette: Palette) -> Self {
        Self {
            width,
            _height: height,
            pixels: vec![0u32; width * height],
            palette,
        }
    }

    pub(crate) fn update(&mut self, grid: &Grid) {
        self.pixels
            .par_chunks_exact_mut(self.width)
            .enumerate()
            .for_each(|(y, pixels)| {
                for (pixel, cell) in pixels.iter_mut().zip(grid.row(y as i32)) {
                    *pixel = self.palette.get_color(cell.level);
                }
            });
        // for (pixel, cell) in self.pixels.iter_mut().zip(cells.iter()) {
        //     *pixel = self.palette.get_color(cell.level);
        // }
    }
}

pub(crate) struct Palette {
    colors: [u32; 256],
    limit: f32,
}

impl Palette {
    pub(crate) fn new(limit: f32) -> Self {
        Self {
            colors: Self::build_palette(),
            limit,
        }
    }

    fn get_color(&self, level: f32) -> u32 {
        // if level == 0.0 {
        //     0x00_ff_00_00
        // } else {
        self.colors[((level.abs().sqrt() / self.limit * 256.0) as usize).clamp(1, 255)]
        // }
    }

    /// Saturated red → yellow → white, black at index 0 (90s demo look).
    fn build_palette() -> [u32; 256] {
        // let red_curve = Curve::new(0.5, 0.5);
        // let green_curve = Curve::new(0.5, 0.5);
        // let blue_curve = Curve::new(0.5, 0.5);
        let mut result = [0; 256];
        for (index, color) in result.iter_mut().enumerate() {
            let t = index as f64 / 255.0;
            // let red = red_curve.get_value(t as f32);
            // let green = green_curve.get_value(t as f32);
            // let blue = blue_curve.get_value(t as f32);

            // let red = (red * 256.0).clamp(0.0, 255.0) as u32;
            // let green = (green * 256.0).clamp(0.0, 255.0) as u32;
            // let blue = (blue * 256.0).clamp(0.0, 255.0) as u32;
            let t = if index % 2 == 0 { t + 0.1 } else { t };
            let red = ((t + 0.0).powf(1.5) * 256.0).clamp(0.0, 255.0) as u32;
            let green = ((t + 0.0).powf(1.3) * 256.0).clamp(0.0, 255.0) as u32;
            let blue = ((t + 0.0).powf(1.0) * 256.0).clamp(0.0, 255.0) as u32;
            // let red = (255.0 * t.powf(0.85)).min(255.0) as u32;
            // let green = (255.0f64 * (t - 0.15).max(0.0) / 0.85).powf(1.1).min(255.0) as u32;
            // let blue = (255.0f64 * (t - 0.45).max(0.0) / 0.55)
            //     .powf(1.25)
            //     .min(255.0) as u32;
            *color = (red << 16) | (green << 8) | blue;
        }
        result
    }
}

// struct Curve {
//     a: f32,
//     b: f32,
//     c: f32,
// }

// impl Curve {
//     fn new(x: f32, y: f32) -> Self {
//         // compute the three coefficients for a parabola through the points (0, 0), (x, y), (1, 1)
//         let a = 2.0 * y / (x * (x - 1.0));
//         let b = -2.0 * y / (x - 1.0);
//         let c = y;
//         Self { a, b, c }
//     }

//     fn get_value(&self, t: f32) -> f32 {
//         self.a * t * t + self.b * t + self.c
//     }
// }
