use crate::{cli::Cli, speech::VOICE};
use color_eyre::eyre::{ContextCompat, ensure};
use moonshine_rs::{
    ModelArch, SttDependenciesOptions, TtsOptions, get_stt_dependencies_with_options,
    get_tts_dependencies,
};
use std::{env, path::Path};
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

    let manifests = [
        get_stt_dependencies_with_options(
            "en",
            &SttDependenciesOptions::new().with_arch(ModelArch::TinyStreaming),
        )?,
        get_tts_dependencies(Some("en"), Some(&TtsOptions::new().with_voice(VOICE)))?,
    ];
    let client = reqwest::Client::new();
    for manifest in manifests {
        download_manifest(&client, &dir, &manifest).await?;
    }

    Ok(())
}

async fn download_manifest(
    client: &reqwest::Client,
    dir: &Path,
    manifest: &str,
) -> color_eyre::Result<()> {
    let manifest: serde_json::Value = serde_json::from_str(manifest)?;

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

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).await?;
            }

            tracing::info!("Fetch {name} <- {url}");
            let bytes = client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            ensure!(
                expected_size == 0 || bytes.len() as u64 == expected_size,
                "Unexpected download size for {name}"
            );
            let mut temporary = dest.as_os_str().to_os_string();
            temporary.push(".part");
            fs::write(&temporary, &bytes).await?;
            fs::rename(&temporary, &dest).await?;
            tracing::info!("Wrote {} bytes to {}", bytes.len(), dest.display());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tts_manifest_includes_only_the_selected_voice_and_phonemizer() {
        let json =
            get_tts_dependencies(Some("en"), Some(&TtsOptions::new().with_voice(VOICE))).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(&json).unwrap();
        let names: Vec<_> = manifest["groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|group| group["files"].as_array().unwrap())
            .map(|file| file["name"].as_str().unwrap())
            .collect();

        assert!(names.contains(&"en_us/g2p-config.json"));
        assert!(names.contains(&"en_us/oov/model.ort"));
        assert!(names.iter().any(|name| name.contains("en_US-ryan-low")));
        assert!(
            names
                .iter()
                .all(|name| { !name.contains("piper-voices/") || name.contains("en_US-ryan-low") })
        );
        assert!(names.iter().all(|name| !name.contains("kokoro")));
    }
}
