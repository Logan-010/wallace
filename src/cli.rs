use clap::Parser;
use std::{num::NonZeroUsize, path::PathBuf};

#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
pub struct Cli {
    /// Set log level
    #[arg(long, short = 'L', env = "RUST_LOG", default_value_t = String::from(concat!(env!("CARGO_PKG_NAME"), "=info,warn")))]
    pub logging: String,

    /// Wallace data path
    #[arg(long, short)]
    pub dir: Option<PathBuf>,

    /// Provider
    #[arg(long, short, env = "WALLACE_PROVIDER")]
    pub provider: String,

    /// Credentials to use
    #[arg(long, short, env = "WALLACE_CREDENTIALS")]
    pub credentials: Option<String>,

    /// Base url for provider
    #[arg(long, short, env = "WALLACE_BASE_URL")]
    pub base_url: Option<String>,

    /// Brave API key
    #[arg(long, short = 'B', env = "BRAVE_API_KEY")]
    pub brave_api_key: Option<String>,

    /// Wake word
    #[arg(long, short, env = "WALLACE_WAKE_WORD", default_value_t = String::from("Wallace"))]
    pub wake_word: String,

    /// Recent messages to use as context; stored history retains at most twice this number
    #[arg(long, env = "WALLACE_MEMORY_LIMIT", default_value = "10")]
    pub memory_limit: NonZeroUsize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_limit_can_be_configured() {
        let cli =
            Cli::try_parse_from(["wallace", "--provider", "test", "--memory-limit", "5"]).unwrap();
        assert_eq!(cli.memory_limit.get(), 5);
    }

    #[test]
    fn memory_limit_must_be_positive() {
        for limit in ["0", "-1", "invalid"] {
            assert!(
                Cli::try_parse_from(["wallace", "--provider", "test", "--memory-limit", limit,])
                    .is_err()
            );
        }
    }
}
