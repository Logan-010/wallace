mod agent;
mod cli;
mod download;
mod speech;
mod transcribe;
mod vad;

use clap::Parser;
use cli::Cli;
use std::sync::LazyLock;
use tokio::{select, signal, task};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    if let Err(e) = tracing_subscriber::registry()
        .with(EnvFilter::new(&cli.logging))
        .with(tracing_subscriber::fmt::layer())
        .try_init()
    {
        eprintln!("Failed to initialize logger: {e:?}");
    }

    download::download(&cli).await?;
    let rx = transcribe::transcribe(&cli).await?;

    let shutdown = signal::ctrl_c();
    let task = task::spawn(agent::task(cli, rx));

    select! {
        exit_res = shutdown => exit_res?,
        task_res = task => task_res??,
    }

    Ok(())
}
