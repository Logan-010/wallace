use crate::cli::Cli;
use tokio::sync::mpsc;

pub async fn task(
    cli: Cli,
    rx: mpsc::Receiver<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    Ok(())
}
