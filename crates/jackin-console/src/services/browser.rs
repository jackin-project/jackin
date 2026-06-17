//! Host browser side-effect adapter for console input.

pub fn open_url(url: &str) -> anyhow::Result<()> {
    open::that_detached(url)?;
    Ok(())
}
