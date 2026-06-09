//! Host browser side-effect adapter for console input.

pub(crate) fn open_url(url: &str) -> anyhow::Result<()> {
    open::that_detached(url)?;
    Ok(())
}
