use crate::cli::Cli;
use color_eyre::eyre::ContextCompat;
use moonshine_rs::{ModelArch, SttDependenciesOptions, get_stt_dependencies_with_options};
use std::env;
use tokio::fs;

pub async fn download(cli: &Cli) -> color_eyre::Result<()> {
    let dir = match cli.dir.as_ref() {
        Some(d) => d.to_path_buf(),
        None => env::home_dir()
            .context("Expected home dir")?
            .join(".wallace")
            .join("models"),
    };

    if !fs::try_exists(&dir).await? {
        fs::create_dir_all(&dir).await?;
    }

    let manifest_json = get_stt_dependencies_with_options(
        "en",
        &SttDependenciesOptions::new().with_arch(ModelArch::TinyStreaming),
    )?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_json)?;

    let groups = manifest["groups"]
        .as_array()
        .context("Manifest missing [groups] ")?;

    for group in groups {
        let files = group["files"].as_array().context("Group missing [files]")?;
        for file in files {
            let name = file["name"].as_str().context("File missing \"name\"")?;
            let url = file["url"].as_str().context("File missing \"url\"")?;
            let expected_size = file["size"].as_u64().unwrap_or(0);

            let dest = dir.join(name);
            if fs::try_exists(&dest).await? {
                let have = fs::metadata(&dest).await?.len();
                if expected_size == 0 || have == expected_size {
                    tracing::debug!("Skip {name} (already present, {have} bytes)");
                    continue;
                }
            }

            tracing::info!("Fetch {name} <- {url}");
            let bytes = reqwest::get(url).await?.bytes().await?;
            fs::write(&dest, &bytes).await?;
            tracing::info!("Wrote {} bytes to {}", bytes.len(), dest.display());
        }
    }

    Ok(())
}
