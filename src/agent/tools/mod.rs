mod cmd;
mod datetime;
mod search;
pub mod speech;
mod weather;

use crate::cli::Cli;
use tokio::{sync::mpsc, task};

pub async fn build(
    cli: &Cli,
) -> color_eyre::Result<(
    search::SearchTool,
    speech::SpeechTool,
    cmd::CmdTool,
    datetime::DateTimeTool,
)> {
    let search = search::SearchTool::new(cli.brave_api_key.clone())?;
    let (speech_tx, speech_rx) = speech::task(cli).await?;
    let speech = speech::SpeechTool::new(speech_tx);
    let cmd = cmd::CmdTool;
    let datetime = datetime::DateTimeTool;

    task::spawn(monitor_speech(speech_rx));

    Ok((search, speech, cmd, datetime))
}

async fn monitor_speech(mut rx: mpsc::Receiver<color_eyre::Result<()>>) {
    while let Some(result) = rx.recv().await {
        if let Err(e) = result {
            tracing::warn!("Speech error: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn speech_monitor_keeps_receiving_after_success_and_errors() {
        let (tx, rx) = mpsc::channel(1);
        let monitor = task::spawn(monitor_speech(rx));

        tx.send(Ok(())).await.unwrap();
        tx.send(Ok(())).await.unwrap();
        tx.send(Err(color_eyre::eyre::eyre!("playback failed")))
            .await
            .unwrap();
        tx.send(Ok(())).await.unwrap();

        drop(tx);
        monitor.await.unwrap();
    }
}
