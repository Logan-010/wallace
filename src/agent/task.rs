use crate::cli::Cli;
use tokio::sync::mpsc;

pub async fn task(
    cli: Cli,
    mut rx: mpsc::Receiver<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    let (speech_tx, speech_rx) = crate::speech::task(&cli).await?;

    Ok(())
}
