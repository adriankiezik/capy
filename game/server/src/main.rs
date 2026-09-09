use capy_server::create_server;

fn main() -> anyhow::Result<()> {
    create_server().run()?;

    Ok(())
}
