//! The part of the simulation which handles the ants that move around the grid and leave pheromones.

use std::{cmp::Ordering, f32::consts::PI};

use crate::{
    config::{AgentConfig, Config, WallBounceReaction},
    random::{rand_f32, rand_symmetric_f32},
    simulation::Simulation,
};

/// Encapsulates a 2D rotation by angle alpha using precomputed cosine and sine values.
#[derive(Debug, Clone, Copy)]
struct Rotation2d {
    /// precomputed cos(alpha).
    cos: f32,

    /// precomputed sin(alpha).
    sin: f32,
}

impl Rotation2d {
    /// Create a rotation representation from an angle in radians.
    fn from_radians(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self { cos, sin }
    }

    /// Rotate a 2D vector direction `[x, y]` using matrix multiplication.
    #[inline]
    fn rotate_vector(self, vector: [f32; 2]) -> [f32; 2] {
        let [x, y] = vector;
        [
            x.mul_add(self.cos, -(y * self.sin)),
            x.mul_add(self.sin, y * self.cos),
        ]
    }
}

/// Takes a raw 2D vector offset and returns a normalized unit vector `[dx, dy]`.
/// If the length is zero or invalid, it gracefully falls back to a default vector.
#[inline]
fn normalize_vector(vector: [f32; 2], fallback: [f32; 2]) -> [f32; 2] {
    let [x, y] = vector;
    let length = x.hypot(y);
    if length == 0.0 || length.is_nan() {
        fallback
    } else {
        [x / length, y / length]
    }
}

/// Current runtime state of an ant that moves around the grid and leaves pheromones.
// #[repr(align(64))]
pub(crate) struct Agent {
    /// The x-coordinate of the agent in pixels.
    pub(crate) x: f32,

    /// The y-coordinate of the agent in pixels.
    pub(crate) y: f32,

    /// The normalized 2D direction vector of the agent.
    ///
    /// [0] faces along the x-axis, [1] faces along the y-axis.
    direction: [f32; 2],

    /// The speed of the agent in pixels per update.
    speed: f32,

    /// The pheromone value of the agent.
    pub(crate) value: f32,

    /// current seed for the random number generator.
    rng: u32,

    /// The width of the sensor cone in radians.
    sensor_width: f32,

    /// Precomputed sniffing positions.
    sniffing_positions: [SniffingPosition; 4],

    /// The distance of the sensor cone in pixels.
    sensor_distance: f32,

    /// Factor by which the speed of anti-ants will be reduced compared to normal ants.
    anti_speed_factor: f32,

    /// Whether to flip the sign of the pheromone value when the agent hits a wall.
    wall_bounce_flip_value: bool,

    /// What to do, if the agent hits a wall.
    wall_bounce_reaction: WallBounceReaction,
}

/// Position at which an agent should "sniff",
/// Consists of a direction and a weight.
struct SniffingPosition {
    /// The direction in which to "sniff".
    direction: Rotation2d,

    /// An arbitrary weight for this "sniffing point".
    weight: f32,
}

impl SniffingPosition {
    /// Precompute the sniffing position from an angle in radians.
    /// The weight will be the equal to the angle itself.
    fn from_radians(radians: f32) -> Self {
        Self {
            direction: Rotation2d::from_radians(radians),
            weight: radians,
        }
    }
}

impl Agent {
    /// Create a new agent with the given configuration.
    pub(crate) fn new(config: &Config, width: u16, height: u16, index: u32) -> Self {
        #![expect(
            clippy::suboptimal_flops,
            reason = "this would impair readability for code that is not performance critical"
        )]

        let AgentConfig {
            count: _,
            ref value,
            ref speed,
            sensor_width,
            sensor_distance,
            anti_percentage,
            anti_speed_factor,
            wall_bounce_flip_value,
            wall_bounce_reaction,
        } = config.agent;

        // compute an individual seed

        // use the index as the seed for the random number generator
        let mut rng = index;
        let mut random = || rand_f32(&mut rng);

        let (width, height) = (f32::from(width), f32::from(height));

        let x = random() * width;
        let y = random() * height;
        let center_distance = f32::hypot(x - width * 0.5, y - height * 0.5);
        let radius = height * 0.1 / center_distance;
        let x = (x - width * 0.5) * radius + width * 0.5;
        let y = (y - height * 0.5) * radius + height * 0.5;

        // Calculate the raw offset from the center flipped by 180 degrees
        let dx = (width * 0.5) - x;
        let dy = (height * 0.5) - y;

        // Normalize using our new encapsulation helper
        let direction = normalize_vector([dx, dy], [1.0, 0.0]);

        let mut speed_seed = index ^ 0x1234_5678;
        let mut value_seed = index ^ 0x8765_4321;
        let speed = speed.start + rand_f32(&mut speed_seed) * (speed.end - speed.start);
        let value = value.start + rand_f32(&mut value_seed) * (value.end - value.start);
        let sign = if random() > anti_percentage {
            1.0
        } else {
            -1.0
        };

        let sniffing_positions = Self::compute_sniffing_positions(sensor_width);

        Self {
            x,
            y,
            direction,
            speed,
            value: value * sign,
            rng,
            sensor_width,
            sniffing_positions,
            sensor_distance,
            anti_speed_factor,
            wall_bounce_flip_value,
            wall_bounce_reaction,
        }
    }

    /// Update the agent's position and direction based on the current simulation state.
    pub(crate) fn update(&mut self, simulation: &Simulation) {
        self.update_direction(simulation);
        self.update_position(simulation);
    }

    /// Update the position of the agent based on its current direction and speed.
    fn update_position(&mut self, simulation: &Simulation) {
        let scale = if self.value > 0.0 {
            1.0
        } else {
            self.anti_speed_factor
        };

        let [dx, dy] = self.direction;
        let new_x = dx.mul_add(self.speed * scale, self.x);
        let new_y = dy.mul_add(self.speed * scale, self.y);

        let (width, height) = (f32::from(simulation.width), f32::from(simulation.height));

        let rate_range = |value: f32, min: f32, max: f32| {
            if value < min {
                Ordering::Less
            } else if value > max {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        };
        let x_rating = rate_range(new_x, 0.0, width);
        let y_rating = rate_range(new_y, 0.0, height);

        let hit_wall = !matches!((x_rating, y_rating), (Ordering::Equal, Ordering::Equal));
        if hit_wall {
            if self.wall_bounce_flip_value {
                self.value = -self.value;
            }
            self.wall_bounce(new_x, new_y, width, height, x_rating, y_rating);
        } else {
            self.x = new_x;
            self.y = new_y;
        }
    }

    /// Update position and maybe direction of the agent when it hits a wall.
    fn wall_bounce(
        &mut self,
        new_x: f32,
        new_y: f32,
        width: f32,
        height: f32,
        x_rating: Ordering,
        y_rating: Ordering,
    ) {
        match self.wall_bounce_reaction {
            WallBounceReaction::Center => (self.x, self.y) = (width / 2.0, height / 2.0),
            WallBounceReaction::Random => {
                self.x = rand_f32(&mut self.rng) * width;
                self.y = rand_f32(&mut self.rng) * height;
            }
            WallBounceReaction::WrapAround => {
                self.x = match x_rating {
                    Ordering::Less => new_x + width,
                    Ordering::Greater => new_x - width,
                    Ordering::Equal => new_x,
                };
                self.y = match y_rating {
                    Ordering::Less => new_y + height,
                    Ordering::Greater => new_y - height,
                    Ordering::Equal => new_y,
                };
            }
            WallBounceReaction::Clip => {
                self.x = match x_rating {
                    Ordering::Less => 0.0,
                    Ordering::Greater => width,
                    Ordering::Equal => new_x,
                };
                self.y = match y_rating {
                    Ordering::Less => 0.0,
                    Ordering::Greater => height,
                    Ordering::Equal => new_y,
                };
            }
            WallBounceReaction::FaceAway(spread) => {
                let mut new_direction = |base_angle: f32| {
                    let angle = rand_symmetric_f32(&mut self.rng).mul_add(spread, base_angle);
                    let (sin, cos) = angle.sin_cos();
                    [cos, sin]
                };

                match x_rating {
                    Ordering::Less => {
                        self.direction = new_direction(0.0);
                        self.x = 0.0;
                    }
                    Ordering::Greater => {
                        self.direction = new_direction(PI);
                        self.x = width;
                    }
                    Ordering::Equal => {
                        self.x = new_x;
                    }
                }
                match y_rating {
                    Ordering::Less => {
                        self.direction = new_direction(0.5 * PI);
                        self.y = 0.0;
                    }
                    Ordering::Greater => {
                        self.direction = new_direction(1.5 * PI);
                        self.y = height;
                    }
                    Ordering::Equal => {
                        self.y = new_y;
                    }
                }
            }
            WallBounceReaction::BounceOff => {
                match x_rating {
                    Ordering::Less | Ordering::Greater => {
                        self.direction[0] = -self.direction[0];
                        self.x = if matches!(x_rating, Ordering::Less) {
                            0.0
                        } else {
                            width
                        };
                    }
                    Ordering::Equal => self.x = new_x,
                }
                match y_rating {
                    Ordering::Less | Ordering::Greater => {
                        self.direction[1] = -self.direction[1];
                        self.y = if matches!(y_rating, Ordering::Less) {
                            0.0
                        } else {
                            height
                        };
                    }
                    Ordering::Equal => self.y = new_y,
                }
                // Ensure floating-point accuracy remains preserved after mirroring components
                self.direction = normalize_vector(self.direction, [1.0, 0.0]);
            }
        }
    }

    /// Update the direction (orientation) of the agent based on pheromone levels around it.
    fn update_direction(&mut self, simulation: &Simulation) {
        let mut angle_sum = 0.0;

        for sniff in &self.sniffing_positions {
            let [rx, ry] = sniff.direction.rotate_vector(self.direction);

            let x = self.sensor_distance.mul_add(rx, self.x);
            let y = self.sensor_distance.mul_add(ry, self.y);

            let level = simulation.read_buffer.cell(x, y).level;
            angle_sum = level.mul_add(sniff.weight, angle_sum);
        }

        // Incorporate steering angle delta and random jitter
        let total_turn =
            (rand_symmetric_f32(&mut self.rng) * self.sensor_width).mul_add(0.3, angle_sum * 0.5);

        // Final directional modification matrix
        let final_rotation = Rotation2d::from_radians(total_turn);
        let raw_new_direction = final_rotation.rotate_vector(self.direction);

        self.direction = raw_new_direction;
        // Normalize the final vector to ensure there is no drift
        // self.direction = normalize_vector(raw_new_direction, self.direction);
    }

    #[expect(clippy::neg_multiply, reason = "readability")]
    /// Precompute the sniffing positions for a given sensor width.
    fn compute_sniffing_positions(sensor_width: f32) -> [SniffingPosition; 4] {
        // Precompute sensory sampling offsets utilizing Rotation2d matrix structures
        [
            SniffingPosition::from_radians(sensor_width * -1.0),
            SniffingPosition::from_radians(sensor_width * -0.5),
            SniffingPosition::from_radians(sensor_width * 0.5),
            SniffingPosition::from_radians(sensor_width * 1.0),
        ]
    }

    /// Update the agent's configuration.
    pub(crate) fn update_config(&mut self, config: &AgentConfig, index: u32) {
        let AgentConfig {
            sensor_width,
            sensor_distance,
            anti_speed_factor,
            wall_bounce_flip_value,
            wall_bounce_reaction,
            count: _, // used for creation/destruction of new agents
            ref value,
            ref speed,
            anti_percentage: _, // used for creation of new agents
        } = *config;
        self.sensor_distance = sensor_distance;
        self.anti_speed_factor = anti_speed_factor;
        self.wall_bounce_flip_value = wall_bounce_flip_value;
        self.wall_bounce_reaction = wall_bounce_reaction;

        self.sensor_width = sensor_width;
        self.sniffing_positions = Self::compute_sniffing_positions(sensor_width);

        let mut speed_seed = index ^ 0x1234_5678;
        self.speed = speed.start + rand_f32(&mut speed_seed) * (speed.end - speed.start);

        // update speed, preserving the sign
        let mut value_seed = index ^ 0x8765_4321;
        let sign = self.value.signum();
        self.value = sign * (value.start + rand_f32(&mut value_seed) * (value.end - value.start));
    }
}
