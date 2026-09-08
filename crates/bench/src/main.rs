mod cli;
mod report;
mod runner;
mod scenario;

use anyhow::Result;
use clap::Parser;
use cli::Options;

fn main() -> Result<()> {
    runner::run(Options::parse())
}
