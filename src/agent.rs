//! The part of the simulation which handles the ants that move around the grid and leave pheromones.

use std::{
    cmp::Ordering,
    f32::consts::{PI, TAU},
};

use crate::{
    config::{AgentConfig, Config, WallBounceReaction},
    random::{rand_f32, rand_symmetric_f32},
    simulation::Simulation,
};

/// Current runtime state of an ant that moves around the grid and leaves pheromones.
pub(crate) struct Agent {
    /// The x-coordinate of the agent in pixels.
    pub(crate) x: f32,

    /// The y-coordinate of the agent in pixels.
    pub(crate) y: f32,

    /// The direction of the agent in radians.
    ///
    /// 0.0 faces towards the positive x-axis. Rotation is clockwise.
    direction: f32,

    /// The speed of the agent in pixels per update.
    speed: f32,

    /// The pheromone value of the agent.
    pub(crate) value: f32,

    /// current seed for the random number generator.
    rng: u32,

    /// The width of the sensor cone in radians.
    sensor_width: f32,

    /// The distance of the sensor cone in pixels.
    sensor_distance: f32,

    /// Factor by which the speed of anti-ants will be reduced compared to normal ants.
    anti_speed_factor: f32,

    /// Whether to flip the sign of the pheromone value when the agent hits a wall.
    wall_bounce_flip_value: bool,

    /// What to do, if the agent hits a wall.
    wall_bounce_reaction: WallBounceReaction,

    /// The amount of jitter (randomness) in the direction of the agent.
    jitter: f32,

    /// Factor of how much the sensor's value affects the direction of the agent.
    sensor_strength: f32,
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
            jitter,
            sensor_strength,
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
        // let (sin, cos) = (direction + PI).sin_cos();
        // let x = width as f32 * 0.5 + cos * r;
        // let y = height as f32 * 0.5 + sin * r;

        // let x = width as f32 * 0.5;
        // let y = height as f32 * 0.5;

        // let direction = random() * TAU;
        let direction = f32::atan2(y - height * 0.5, x - width * 0.5) - PI * 1.0;
        let direction = if direction.is_nan() { 0.0 } else { direction };
        // let direction = PI / 2.0;

        let mut speed_seed = index ^ 0x1234_5678;
        let mut value_seed = index ^ 0x8765_4321;
        let speed = speed.start + rand_f32(&mut speed_seed) * (speed.end - speed.start);
        let value = value.start + rand_f32(&mut value_seed) * (value.end - value.start);
        let sign = if random() > anti_percentage {
            1.0
        } else {
            -1.0
        };

        Self {
            x,
            y,
            direction,
            speed,
            value: value * sign,
            rng,
            sensor_width,
            sensor_distance,
            anti_speed_factor,
            wall_bounce_flip_value,
            wall_bounce_reaction,
            jitter,
            sensor_strength,
        }
    }

    /// Update the agent's position and direction based on the current simulation state.
    pub(crate) fn update(&mut self, simulation: &Simulation) {
        self.update_direction(simulation);
        self.update_position(simulation);
    }

    /// Update the position of the agent based on its current direction and speed.
    ///
    /// This method will also handle wall collisions and the wall bounce reaction which might affect
    /// the direction and value.
    fn update_position(&mut self, simulation: &Simulation) {
        // make anti-ants slower than normal ants
        let scale = if self.value > 0.0 {
            1.0
        } else {
            self.anti_speed_factor
        };

        // move ant into the direction it is facing
        let (sin, cos) = self.direction.sin_cos();
        let new_x = cos.mul_add(self.speed * scale, self.x);
        let new_y = sin.mul_add(self.speed * scale, self.y);

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
                let mut new_direction =
                    |normal: f32| normal.mul_add(TAU, rand_symmetric_f32(&mut self.rng) * spread);

                match x_rating {
                    Ordering::Less => {
                        self.direction = new_direction(0.0);
                        self.x = 0.0;
                    }
                    Ordering::Greater => {
                        self.direction = new_direction(0.5);
                        self.x = width;
                    }
                    Ordering::Equal => {
                        self.x = new_x;
                    }
                }
                match y_rating {
                    Ordering::Less => {
                        self.direction = new_direction(0.25);
                        self.y = 0.0;
                    }
                    Ordering::Greater => {
                        self.direction = new_direction(0.75);
                        self.y = height;
                    }
                    Ordering::Equal => {
                        self.y = new_y;
                    }
                }
            }
            WallBounceReaction::BounceOff => {
                match x_rating {
                    Ordering::Less => {
                        self.direction = PI - self.direction;
                        self.x = 0.0;
                    }
                    Ordering::Greater => {
                        self.direction = PI - self.direction;
                        self.x = width;
                    }
                    Ordering::Equal => {
                        self.x = new_x;
                    }
                }
                match y_rating {
                    Ordering::Less => {
                        self.direction = -self.direction;
                        self.y = 0.0;
                    }
                    Ordering::Greater => {
                        self.direction = -self.direction;
                        self.y = height;
                    }
                    Ordering::Equal => {
                        self.y = new_y;
                    }
                }
            }
        }
    }

    /// Update the direction (orientation) of the agent based on pheromone levels around it.
    fn update_direction(&mut self, simulation: &Simulation) {
        let sniff = |angle: f32| {
            let (sin, cos) = angle.sin_cos();
            simulation
                .read_buffer
                .cell(
                    self.sensor_distance.mul_add(cos, self.x),
                    self.sensor_distance.mul_add(sin, self.y),
                )
                .level
        };

        #[expect(clippy::neg_multiply, reason = "improves readability")]
        let sniffs = [
            self.sensor_width * -1.0,
            self.sensor_width * -0.5,
            self.sensor_width * 0.5,
            self.sensor_width * 1.0,
        ];

        let mut angle_sum = 0.0;
        for angle in sniffs {
            angle_sum += sniff(self.direction + angle) * angle;
        }

        let delta = angle_sum;
        let jitter = rand_symmetric_f32(&mut self.rng) * self.sensor_width;
        self.direction += delta * self.sensor_strength + jitter * self.jitter;
    }

    /// Update the agent's configuration.
    ///
    /// Note: some values cannot be changed at runtime and will be ignored.
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
            jitter,
            sensor_strength,
        } = *config;
        self.sensor_width = sensor_width;
        self.sensor_distance = sensor_distance;
        self.anti_speed_factor = anti_speed_factor;
        self.wall_bounce_flip_value = wall_bounce_flip_value;
        self.wall_bounce_reaction = wall_bounce_reaction;
        self.jitter = jitter;
        self.sensor_strength = sensor_strength;

        let mut speed_seed = index ^ 0x1234_5678;
        self.speed = speed.start + rand_f32(&mut speed_seed) * (speed.end - speed.start);

        // update speed, preserving the sign
        let mut value_seed = index ^ 0x8765_4321;
        let sign = self.value.signum();
        self.value = sign * (value.start + rand_f32(&mut value_seed) * (value.end - value.start));
    }
}
