use crate::{agent::util, cli::Cli};
use color_eyre::eyre::ContextCompat;
use rig::sqlite::SqliteSessionStore;
use std::env;
use tokio::sync::mpsc;
use tokio_rusqlite::Connection;

pub async fn task(
    cli: Cli,
    mut rx: mpsc::Receiver<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    let dir = match cli.dir.as_ref() {
        Some(d) => d.to_path_buf(),
        None => env::home_dir()
            .context("Expected home dir")?
            .join(".wallace")
            .join("memory.db"),
    };

    let model = util::connect_at(
        &cli.provider,
        cli.credentials.as_deref(),
        cli.base_url.as_deref(),
    )
    .await?;

    let conn = Connection::open(dir).await?;
    let memory = SqliteSessionStore::open(conn).await?;

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
