use clap::Parser;
use std::path::PathBuf;

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
}
