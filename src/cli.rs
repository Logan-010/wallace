use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
#[value(rename_all = "kebab-case")]
pub enum ModelArch {
    Tiny,
    Base,
    #[value(alias = "tinystreaming")]
    TinyStreaming,
    #[default]
    #[value(alias = "smallstreaming")]
    SmallStreaming,
    #[value(alias = "mediumstreaming")]
    MediumStreaming,
    #[value(alias = "basestreaming")]
    BaseStreaming,
}

impl From<ModelArch> for moonshine_rs::ModelArch {
    fn from(value: ModelArch) -> Self {
        match value {
            ModelArch::Tiny => Self::Tiny,
            ModelArch::Base => Self::Base,
            ModelArch::TinyStreaming => Self::TinyStreaming,
            ModelArch::SmallStreaming => Self::SmallStreaming,
            ModelArch::MediumStreaming => Self::MediumStreaming,
            ModelArch::BaseStreaming => Self::BaseStreaming,
        }
    }
}

#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
pub struct Cli {
    /// Set log level
    #[arg(long, short = 'L', env = "RUST_LOG", default_value_t = String::from(concat!(env!("CARGO_PKG_NAME"), "=info,warn")))]
    pub logging: String,

    /// Language to use for the model
    #[arg(long, short, default_value = "en")]
    pub language: String,

    /// Model arch to use
    #[arg(long, short, value_enum, default_value_t = ModelArch::Tiny)]
    pub arch: ModelArch,

    /// Path to download models to
    #[arg(long, short)]
    pub dir: Option<PathBuf>,
}
