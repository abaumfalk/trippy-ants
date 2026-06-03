//! The configuration for the simulation.

#![allow(unused, reason = "not all configuration options are used all the time")]
use std::{
    f32::consts::PI,
    fs,
    ops::Range,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};

/// The configuration preset that is used if no config file is provided.
pub(crate) const DEFAULT_CONFIG: Config = Config {
    world: WorldConfig {
        wall_value: None,
        topology: GridTopology::Plane,
        decay_factor: 0.99,
    },
    agent: AgentConfig {
        count: 20_000,
        value: 0.3..1.2,
        speed: 1.0..1.7,
        sensor_distance: 20.0,
        sensor_width: 0.6,
        anti_percentage: 0.0,
        anti_speed_factor: 0.1,
        wall_bounce_flip_value: true,
        wall_bounce_reaction: WallBounceReaction::Center,
    },
    colors: ColorConfig {
        normal: [1.0, -1.0, -0.3],
        anti: [-1.5, -1.0, 0.0],
    },
};

/// The configuration-preset for a simulation.
#[derive(Serialize, Deserialize)]
pub(crate) struct Config {
    /// Configures the pheromone dispersion on the grid..
    pub(crate) world: WorldConfig,

    /// Configures the ants that will move around the grid and leave pheromones.
    pub(crate) agent: AgentConfig,

    /// Configures the colorization of the pheromone levels.
    pub(crate) colors: ColorConfig,
}

/// The topology of the grid the simulation will use.
///
/// This will mostly affect the blur-effect of the edges of the grid. The Torus topology will allow
/// for the pheromones levels to blur into the opposite edge, while the Plane topology will clip
/// the them at the edge of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum GridTopology {
    /// The grid is a torus. Data from one edge will blur into the opposite edge.
    ///
    /// Use this if you want to stitch the screenshot together to form a seamless image.
    Torus,
    /// The grid is a plane. Data will not blur into the opposite edge but will be clipped.
    Plane,
}

/// Configuration for the ants that will move around the grid and leave pheromones.
#[derive(Serialize, Deserialize)]
pub(crate) struct AgentConfig {
    /// Number of ants.
    pub(crate) count: u32,

    /// Minimum and maximum pheromone intensity an ant can leave on the grid.
    #[serde(with = "serde_range_f32")]
    pub(crate) value: Range<f32>,

    /// Minimum and maximum speed of an ant.
    #[serde(with = "serde_range_f32")]
    pub(crate) speed: Range<f32>,

    /// How many pixels ahead of the ant it will try to sense the pheromones of other ants.
    pub(crate) sensor_distance: f32,

    /// The angle in radians that the ant will look left and right to sense the pheromones of other ants.
    ///
    /// The higher the value the sharper the turns the ant will take to follow the trail.
    ///
    /// - 0.0 means that the ant will only sense the pheromones directly ahead.
    /// - PI/2 (~1.5) means that the ant will look directly left and right.
    /// - PI (~3.1) means that the ant will look all around.
    pub(crate) sensor_width: f32,

    /// Fraction of ants that will be spawned as anti-ants.
    ///
    /// - 0.0 means that no anti-ants will be spawned.
    /// - 1.0 means that all ants will be spawned as anti-ants.
    /// - 0.5 means that half of the ants will be spawned as anti-ants.
    pub(crate) anti_percentage: f32,

    /// Factor by which the speed of anti-ants will be reduced.
    ///
    /// - 0.0 means that anti-ants won't move.
    /// - 1.0 means that anti-ants will move at the same speed as normal ants.
    /// - 0.5 means that anti-ants will move at half the speed of normal ants.
    pub(crate) anti_speed_factor: f32,

    /// Whether to flip the sign of the pheromone value when the ant hits a wall.
    pub(crate) wall_bounce_flip_value: bool,

    /// Determines what happens when the ant hits a wall.
    pub(crate) wall_bounce_reaction: WallBounceReaction,
}

/// Determines what happens when the ant hits a wall (i.e. tries to leave the window).
#[derive(Clone, Copy, Serialize, Deserialize)]
pub(crate) enum WallBounceReaction {
    /// The ant will respawn at the center of the grid.
    Center,

    /// The ant will respawn at a random position on the grid.
    Random,

    /// The ant will wrap teleport to the opposite edge of the grid.
    ///
    /// It is recommended to use this with [`GridTopology::Torus`].
    WrapAround,

    /// The ant will be stay at the edge of the grid.
    Clip,

    /// The ant will face away from the wall. The given angle determines the possible deviation from the wall's normal.
    ///
    /// - 0.0 means that the ant will face directly away from the wall.
    /// - PI/2 (~1.5) means that the ant will chose any random angle that is facing away from the wall.
    FaceAway(f32),

    /// The ant will bounce off the wall.
    BounceOff,
}

/// Configuration for the world the simulation will run in.
///
/// The settings mostly affect the pheromone dispersion on the grid.
#[derive(Serialize, Deserialize)]
pub(crate) struct WorldConfig {
    /// Constant value for the outermost pixel rows and columns.
    ///
    /// Can be used to repel or attract the ants.
    pub(crate) wall_value: Option<f32>,

    /// Determines how the edge of the frame will be handled.
    pub(crate) topology: GridTopology,

    /// Determines how much of the pheromones will remain after each frame.
    ///
    /// - 0.999 means that 99.9% of the pheromones will remain after each frame.
    /// - 0.99 means that 99% of the pheromones will remain after each frame.
    pub(crate) decay_factor: f32,
}

/// Configuration for the colorization of the pheromone levels.
#[derive(Serialize, Deserialize)]
pub(crate) struct ColorConfig {
    /// Red, Green, Blue values for the normal ants.
    ///
    /// A value of 0.0 means that the color channel will be a linear mapping.
    /// If all values are 0.0, the resulting gradient will be grayscale.
    ///
    /// Greater values will emphasize the color channel and negative values will reduce their effect.
    /// The typical range is -1.0..=1.0.
    pub(crate) normal: [f32; 3],

    /// Red, Green, Blue values for the anti-ants.
    ///
    /// A value of 0.0 means that the color channel will be a linear mapping.
    /// If all values are 0.0, the resulting gradient will be grayscale.
    ///
    /// Greater values will emphasize the color channel and negative values will reduce their effect.
    /// The typical range is -1.0..=1.0.
    pub(crate) anti: [f32; 3],
}

/// Used to be able to serialize and deserialize the range<f32> type as a two-element array rather
/// than a struct with two separate fields.
mod serde_range_f32 {
    use serde::{Deserialize as _, Deserializer, Serialize as _, Serializer};
    use std::ops::Range;

    /// Serialize the range<f32> type as a two-element array.
    ///
    /// # Errors
    ///
    /// Returns an error if the range cannot be serialized.
    pub(crate) fn serialize<S>(range: &Range<f32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [range.start, range.end].serialize(serializer)
    }

    /// Deserialize the range<f32> type from a two-element array.
    ///
    /// # Errors
    ///
    /// Returns an error if the range cannot be deserialized.
    pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Range<f32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [start, end] = <[f32; 2]>::deserialize(deserializer)?;
        Ok(start..end)
    }
}

/// Loads configuration files and keeps watching for updates.
pub(crate) struct ConfigWatcher {
    /// The path to the configuration file.
    ///
    /// If left empty, no configuration will be loaded.
    path: String,

    /// The last modified time of the configuration file.
    ///
    /// If `None`, no configuration has been loaded yet and the watcher will not watch for updates.
    modified: Option<SystemTime>,

    /// The timestamp of the last successful load.
    ///
    /// Used to limit the file system polling frequency.
    timestamp: Instant,
}

impl ConfigWatcher {
    /// Create a new configuration watcher without any active configuration file.
    pub(crate) fn new() -> Self {
        Self {
            path: String::new(),
            modified: None,
            timestamp: Instant::now(),
        }
    }

    /// Load a configuration file from the given path.
    ///
    /// On success, the watcher will be configured for watching future updates.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration file cannot be read or parsed.
    pub(crate) fn load_config(&mut self, path: String) -> Result<Config, String> {
        println!("loading config from '{path}'");
        let config_str = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read config file '{path}': {error}"))?;

        // parse config from string
        let config = toml::from_str(&config_str)
            .map_err(|error| format!("failed to parse config file '{path}': {error}"))?;

        if let Ok(metadata) = fs::metadata(&path)
            && let Ok(modified) = metadata.modified()
        {
            self.modified = Some(modified);
            self.timestamp = Instant::now();
        }
        self.path = path;
        Ok(config)
    }

    /// Check if the configuration file has been updated since the last successful load.
    ///
    /// If the file has been updated, the new configuration will be loaded and returned.
    ///
    /// Returns `None` if …
    ///
    /// - the watcher is not configured
    /// - the cool-down period has not elapsed yet
    /// - the file has not been updated
    /// - any other error occurs (file I/O, parsing, etc.)
    pub(crate) fn watch_for_update(&mut self) -> Option<Config> {
        // skip watching if there wasn't a successful load, yet
        let modified = self.modified?;

        // limit file system polling to once per second
        if self.timestamp.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.timestamp = Instant::now();

        // check whether the file has been modified; cancel on error
        let path = &self.path;
        let metadata = fs::metadata(path).ok()?;
        let modified_new = metadata.modified().ok()?;
        if modified_new <= modified {
            // file has not been modified, skip
            return None;
        }

        // try to load the config file again; cancel on error
        let config_str = fs::read_to_string(path).ok()?;
        let config = toml::from_str(&config_str).ok()?;

        // remember the time of the successful load
        self.modified = Some(modified_new);

        Some(config)
    }
}
