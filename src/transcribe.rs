use crate::{TOKEN, cli::Cli};
use color_eyre::eyre::ContextCompat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use moonshine_rs::{ModelArch, Transcriber};
use std::{
    env,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{select, sync::mpsc, task};

pub async fn transcribe(
    cli: &Cli,
) -> color_eyre::Result<mpsc::Receiver<color_eyre::Result<String>>> {
    let dir = match cli.dir.as_ref() {
        Some(d) => d.to_path_buf(),
        None => env::home_dir()
            .context("Expected home dir")?
            .join(".wallace")
            .join("models"),
    };

    let arch = cli.arch.into();
    let (tx, rx) = mpsc::channel(100);

    task::spawn(async move {
        select! {
            _ = TOKEN.cancelled() => {}
            res = task::spawn_blocking(move || blocking_task(dir, arch, tx)) => if let Err(e) = res {
                tracing::error!("Worker exited: {e:?}");
            }
        }
    });

    Ok(rx)
}

fn blocking_task(
    dir: PathBuf,
    arch: ModelArch,
    tx: mpsc::Sender<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    let ts = Transcriber::from_files(dir, arch, None)?;
    let mut stream = ts.create_stream()?;

    let host = cpal::default_host();
    let device = host.default_input_device().context("No microphone found")?;

    let supported = device
        .supported_input_configs()?
        .find(|config| config.sample_format() == cpal::SampleFormat::F32)
        .context("Microphone does not support f32 audio")?
        .try_with_standard_sample_rate()
        .context("No supported standard sample rate")?;

    let sample_rate = supported.sample_rate();
    let channels = supported.channels() as usize;
    let config = supported.into();

    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(32);

    let input = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let mono: Vec<f32> = data
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();

            let _ = tx.try_send(mono);
        },
        |err| tracing::warn!("Microphone error: {err}"),
        None,
    )?;

    input.play()?;

    tracing::info!("Listening at {sample_rate} Hz...");

    let mut last_update = Instant::now();
    let mut last_text = String::new();

    loop {
        let samples = rx.recv()?;

        stream.add_audio(&samples, sample_rate)?;

        if last_update.elapsed() >= Duration::from_millis(250) {
            let transcript = stream.poll(false)?;

            for line in &transcript.lines {
                if line.is_updated || line.is_new {
                    let status = if line.is_complete { "FINAL" } else { "PARTIAL" };
                    println!(
                        "[{status}] line {}: [{:.2}s - {:.2}s] {}",
                        line.id,
                        line.start_time,
                        line.start_time + line.duration,
                        line.text
                    );
                }
            }

            if text != last_text {
                println!("{text}");
                last_text = text;
            }

            last_update = Instant::now();
        }
    }
}
