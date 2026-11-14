use crate::cli::Cli;
use color_eyre::eyre::ContextCompat;
use tokio::sync::mpsc;

pub async fn task(
    cli: Cli,
    mut rx: mpsc::Receiver<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    let (speech_tx, mut speech_rx) = crate::speech::task(&cli).await?;

    while let Some(text) = rx.recv().await {
        speech_tx.send(text?).await?;
        speech_rx
            .recv()
            .await
            .context("Speech task stopped before playback completed")??;
    }

    Ok(())
}
