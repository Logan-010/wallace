use crate::{
    agent::{memory, tools, util},
    cli::Cli,
};
use color_eyre::eyre::ContextCompat;
use rig::{AgentBuilder, sqlite::SqliteSessionStore};
use std::{env, time::Instant};
use tokio::sync::mpsc;
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
    memory::prune(&memory, cli.memory_limit.get()).await?;

    let (search, speech, cmd, datetime) = tools::build(&cli).await?;

    let agent = AgentBuilder::new(model)
        .name("Wallace")
        .preamble(PROMPT)
        .tool(search)
        .tool(speech)
        .tool(cmd)
        .tool(datetime)
        .default_max_turns(15)
        .build();

    let ww = cli.wake_word.to_lowercase();

    while let Some(text) = rx.recv().await {
        match text {
            Ok(t) => {
                if t.to_lowercase().contains(&ww) {
                    let pause = crate::transcribe::pause();
                    // No previously recognized utterance should run after this request.
                    while let Ok(pending) = rx.try_recv() {
                        if let Err(e) = pending {
                            tracing::warn!("STT error: {e:?}");
                        }
                    }
                    tracing::info!("Prompting agent: {t}");
                    let started = Instant::now();
                    let mut conversation = rig::memory::load_async(&memory, memory::LOG).await?;
                    let history = rig::memory::recent::view(
                        conversation.messages(),
                        &rig::memory::recent::LastMessages(cli.memory_limit.get()),
                    );
                    match agent.prompt(t).history(history.iter().cloned()).await {
                        Ok(response) => {
                            tracing::debug!("Agent response: {response:?}");
                            for message in response.messages {
                                conversation.push(message);
                            }
                            rig::memory::save_async(&memory, memory::LOG, &mut conversation)
                                .await?;
                            memory::prune(&memory, cli.memory_limit.get()).await?;
                            tracing::info!(
                                elapsed_ms = started.elapsed().as_millis(),
                                "Agent run finished; ready for the next query"
                            );
                        }
                        Err(e) => tracing::warn!("Agent error: {e:?}"),
                    }
                    drop(pause);
                } else {
                    tracing::debug!("Skipping non-wake speech: {t}");
                }
            }
            Err(e) => tracing::warn!("STT error: {e:?}"),
        }
    }

    Ok(())
}
