use crate::{agent::util, cli::Cli, speech};
use color_eyre::eyre::ContextCompat;
use rig::{AgentBuilder, sqlite::SqliteSessionStore};
use std::env;
use tokio::{sync::mpsc, task};
use tokio_rusqlite::Connection;

const PROMPT: &str = include_str!("prompt.txt");

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

    let (speech_tx, mut speech_rx) = speech::task(&cli).await?;

    task::spawn(async move {
        while let Some(Err(e)) = speech_rx.recv().await {
            tracing::warn!("Speech error: {e:?}");
        }
    });

    let speech_tool = speech::SpeechTool::new(speech_tx);

    let agent = AgentBuilder::new(model)
        .name("Wallace")
        .preamble(PROMPT)
        .memory(memory)
        .tool(speech_tool)
        .build();

    while let Some(text) = rx.recv().await {
        match text {
            Ok(t) => {
                tracing::info!("Prompting agent: {t}");
                if let Err(e) = agent.prompt(t).await {
                    tracing::warn!("Agent error: {e:?}");
                }
            }
            Err(e) => tracing::warn!("TTS error: {e:?}"),
        }
    }

    Ok(())
}
