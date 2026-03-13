use anyhow::Context;

fn main() -> anyhow::Result<()> {
    capy_engine::run().context("failed to run engine")
}
