mod cmd;
mod data;
mod datetime;
mod search;
pub mod speech;
mod timer;
mod weather;

use crate::cli::Cli;
use tokio::{sync::mpsc, task};
use tokio_rusqlite::Connection;

pub async fn build(
    cli: &Cli,
    db: &Connection,
) -> color_eyre::Result<(
    search::SearchTool,
    speech::SpeechTool,
    cmd::CmdTool,
    datetime::DateTimeTool,
    weather::WeatherTool,
)> {
    data::migrate(db).await?;

    let search = search::SearchTool::new(cli.brave_api_key.clone())?;
    let (speech_tx, speech_rx) = speech::task(cli).await?;
    let speech = speech::SpeechTool::new(speech_tx.clone());
    let cmd = cmd::CmdTool;
    let datetime = datetime::DateTimeTool;
    let weather = weather::WeatherTool::new()?;
    let (timer, timer_rx) = timer::TimersTool::new();
    let (reminders, reminder_rx) = data::RemindersTool::new(db);
    let notes = data::NotesTool::new(db);

    task::spawn(monitor_speech(speech_rx));
    task::spawn(monitor_timers(speech_tx.clone(), timer_rx));
    task::spawn(monitor_reminders(speech_tx, reminder_rx));

    Ok((search, speech, cmd, datetime, weather))
}

async fn monitor_speech(mut rx: mpsc::Receiver<color_eyre::Result<()>>) {
    while let Some(result) = rx.recv().await {
        if let Err(e) = result {
            tracing::warn!("Speech error: {e:?}");
        }
    }
}

async fn monitor_timers(speech_tx: mpsc::Sender<String>, mut timer_rx: mpsc::Receiver<String>) {
    while let Some(timer) = timer_rx.recv().await {
        if speech_tx
            .send(format!("Your timer for {} is going off.", timer))
            .await
            .is_err()
        {
            tracing::warn!("Failed to send timer to TTS");
        }
    }
}

async fn monitor_reminders(
    speech_tx: mpsc::Sender<String>,
    mut reminder_rx: mpsc::Receiver<String>,
) {
    while let Some(reminder) = reminder_rx.recv().await {
        if speech_tx
            .send(format!("Here's your reminder: {}.", reminder))
            .await
            .is_err()
        {
            tracing::warn!("Failed to send timer to TTS");
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
