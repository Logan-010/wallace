mod cli;
mod download;
mod transcribe;
mod vad;

use clap::Parser;
use cli::Cli;
use std::sync::LazyLock;
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
    let mut rx = transcribe::transcribe(&cli).await?;

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    let mut stopping = false;
    loop {
        tokio::select! {
            signal = &mut shutdown, if !stopping => {
                TOKEN.cancel();
                signal?;
                stopping = true;
            }
            result = rx.recv() => match result {
                Some(Ok(text)) => println!("{text}"),
                Some(Err(error)) => {
                    TOKEN.cancel();
                    return Err(error);
                }
                None => break,
            }
        }
    }

    Ok(())
}
