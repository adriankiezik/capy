#[derive(Clone, Debug)]
pub struct CityConfig {
    pub visuals: engine::replica::VisualSettings,
    pub blocks: i32,
    pub leaf_edge: i32,
}

impl Default for CityConfig {
    fn default() -> Self {
        Self {
            visuals: engine::replica::VisualSettings {
                view_distance: 600.0,
                fog_distance: 400.0,
                ..Default::default()
            },
            blocks: 9,
            leaf_edge: 32,
        }
    }
}

#[cfg(feature = "render-bench")]
use {clap::Parser, std::path::PathBuf};

#[cfg(feature = "render-bench")]
#[derive(Parser)]
pub struct MeasurementOptions {
    #[arg(long)]
    pub opaque_parity: bool,
    #[arg(long)]
    pub pose_frame: Option<u32>,
    #[arg(long, default_value_t = 9)]
    pub blocks: i32,
    #[arg(long, default_value_t = 32)]
    pub leaf_edge: i32,
    #[arg(long)]
    pub moving: bool,
    #[arg(long)]
    pub orbit: bool,
    #[arg(long)]
    pub inside: bool,
    #[arg(long, default_value_t = 2940)]
    pub width: u32,
    #[arg(long, default_value_t = 1846)]
    pub height: u32,
    #[arg(long, default_value_t = 200)]
    pub warmup: u32,
    #[arg(long, default_value_t = 600)]
    pub frames: u32,
    #[arg(long)]
    pub capture: Option<PathBuf>,
    #[arg(long)]
    pub legacy_snapshot: Option<PathBuf>,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long)]
    pub no_timestamps: bool,
}
