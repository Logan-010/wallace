use crate::{
    TOKEN,
    agent::tools::speech::PLAYBACK,
    cli::Cli,
    vad::{Event, Vad},
};
use color_eyre::eyre::ContextCompat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use moonshine_rs::{ModelArch, Transcriber};
use std::{
    env,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task};

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

    let (tx, rx) = mpsc::channel(100);

    task::spawn(async move {
        let worker_tx = tx.clone();
        let result = task::spawn_blocking(move || blocking_task(dir, worker_tx)).await;
        let error = match result {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error,
            Err(error) => error.into(),
        };
        let _ = tx.send(Err(error)).await;
    });

    Ok(rx)
}

fn blocking_task(
    dir: PathBuf,
    tx: mpsc::Sender<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    let ts = Transcriber::from_files(dir, ModelArch::TinyStreaming, None)?;
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

    let mut vad = Vad::new(sample_rate)?;
    let (audio_tx, audio_rx) =
        std::sync::mpsc::sync_channel::<color_eyre::Result<(u64, Vec<f32>)>>(32);
    let error_tx = audio_tx.clone();

    let input = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let Ok(generation) = PLAYBACK.try_read() else {
                return;
            };
            let mono: Vec<f32> = data
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();

            let _ = audio_tx.try_send(Ok((*generation, mono)));
        },
        move |err| {
            let _ = error_tx.try_send(Err(err.into()));
        },
        None,
    )?;

    input.play()?;

    tracing::info!("Listening at {sample_rate} Hz...");

    let mut last_update = Instant::now();

    let mut has_audio = false;
    let mut generation = 0;
    while !TOKEN.is_cancelled() && !tx.is_closed() {
        let Ok(playback) = PLAYBACK.try_read() else {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        if generation != *playback {
            vad = Vad::new(sample_rate)?;
            stream = ts.create_stream()?;
            has_audio = false;
            last_update = Instant::now();
            generation = *playback;
        }
        let (captured_generation, samples) = match audio_rx.recv_timeout(Duration::from_millis(100))
        {
            Ok(samples) => samples?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => return Err(error.into()),
        };
        if captured_generation != generation {
            continue;
        }
        for event in vad.push(&samples)? {
            match event {
                Event::Audio(audio) => {
                    stream.add_audio(&audio, sample_rate)?;
                    has_audio = true;
                }
                Event::End => {
                    publish(stream.finalize()?.text(), &tx)?;
                    stream = ts.create_stream()?;
                    has_audio = false;
                }
            }
        }

        if has_audio && last_update.elapsed() >= Duration::from_millis(250) {
            // Keep inference incremental, but only publish finalized utterances.
            stream.poll(false)?;

            last_update = Instant::now();
        }
    }
    drop(input);
    Ok(())
}

fn publish(text: String, tx: &mpsc::Sender<color_eyre::Result<String>>) -> color_eyre::Result<()> {
    if !text.trim().is_empty() {
        tx.blocking_send(Ok(text))?;
    }
    Ok(())
}
