use crate::scenario::Kind;
use clap::Parser;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
const WIDTH: u32 = 1280;

#[cfg(target_os = "macos")]
const HEIGHT: u32 = 832;

#[cfg(not(target_os = "macos"))]
const WIDTH: u32 = 2560;

#[cfg(not(target_os = "macos"))]
const HEIGHT: u32 = 1440;

#[derive(Parser)]
#[command(
    about = "Deterministic offscreen gameplay benchmarks; measures throughput, not display FPS"
)]
pub struct Options {
    #[arg(long, value_enum)]
    pub scenario: Vec<Kind>,
    #[arg(long, default_value_t = 600, value_parser = clap::value_parser!(u32).range(1..=3600))]
    pub frames: u32,
    #[arg(long, default_value_t = 120, value_parser = clap::value_parser!(u32).range(1..=3600))]
    pub warmup: u32,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..=20))]
    pub runs: u32,
    #[arg(long, default_value_t = WIDTH, value_parser = clap::value_parser!(u32).range(1..))]
    pub width: u32,
    #[arg(long, default_value_t = HEIGHT, value_parser = clap::value_parser!(u32).range(1..))]
    pub height: u32,
    #[arg(long)]
    pub output: Option<PathBuf>,
}
