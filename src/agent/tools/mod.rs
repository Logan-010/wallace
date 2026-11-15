mod cmd;
mod search;
pub mod speech;

use crate::cli::Cli;
use tokio::task;

pub async fn build(
    cli: &Cli,
) -> color_eyre::Result<(search::SearchTool, speech::SpeechTool, cmd::CmdTool)> {
    let search = search::SearchTool::new(cli.brave_api_key.clone())?;
    let (speech_tx, mut speech_rx) = speech::task(cli).await?;
    let speech = speech::SpeechTool::new(speech_tx);
    let cmd = cmd::CmdTool;

    task::spawn(async move {
        while let Some(Err(e)) = speech_rx.recv().await {
            tracing::warn!("Speech error: {e:?}");
        }
    });

    Ok((search, speech, cmd))
}
