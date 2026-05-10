pub(crate) const DEFAULT_CONFIG: Config = CONFIG;

const CONFIG: Config = Config {
    agent_count: 20_000,
    limit: 1.0,
    sensor_width: 0.6,
    sensor_distance: 20.0,
    enable_walls: false,

    grid_topology: GridTopology::Torus,
    decay_factor: 0.99,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GridTopology {
    /// The grid is a torus. Data from one edge will blur into the opposite edge.
    ///
    /// Use this if you want to stitch the screenshot together to form a seamless image.
    Torus,
    /// The grid is a plane. Data will not blur into the opposite edge but will be clipped.
    Plane,
}

pub(crate) struct Config {
    // Number of ants
    pub(crate) agent_count: usize,

    pub(crate) limit: f32,
    pub(crate) sensor_width: f32,
    pub(crate) sensor_distance: f32,
    pub(crate) enable_walls: bool,

    /// Determines how the edge of the frame will be handled.
    pub(crate) grid_topology: GridTopology,

    /// Determines how much of the pheromones will remain after each frame.
    ///
    /// - 0.999 means that 99.9% of the pheromones will remain after each frame.
    /// - 0.99 means that 99% of the pheromones will remain after each frame.
    pub(crate) decay_factor: f32,
}
