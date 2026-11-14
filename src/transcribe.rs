use crate::{
    TOKEN,
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

    let arch = cli.arch.into();
    let (tx, rx) = mpsc::channel(100);

    task::spawn(async move {
        let worker_tx = tx.clone();
        let result = task::spawn_blocking(move || blocking_task(dir, arch, worker_tx)).await;
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

    let mut vad = Vad::new(sample_rate)?;
    let (audio_tx, audio_rx) = std::sync::mpsc::sync_channel::<color_eyre::Result<Vec<f32>>>(32);
    let error_tx = audio_tx.clone();

    let input = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let mono: Vec<f32> = data
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();

            let _ = audio_tx.try_send(Ok(mono));
        },
        move |err| {
            let _ = error_tx.try_send(Err(err.into()));
        },
        None,
    )?;

    input.play()?;

    tracing::info!("Listening at {sample_rate} Hz...");

    let mut last_update = Instant::now();
    let mut last_text = String::new();

    let mut has_audio = false;
    while !TOKEN.is_cancelled() && !tx.is_closed() {
        let samples = match audio_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(samples) => samples?,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => return Err(error.into()),
        };
        for event in vad.push(&samples)? {
            match event {
                Event::Audio(audio) => {
                    stream.add_audio(&audio, sample_rate)?;
                    has_audio = true;
                }
                Event::End => {
                    publish(stream.finalize()?.text(), &mut last_text, &tx)?;
                    stream = ts.create_stream()?;
                    has_audio = false;
                    last_text.clear();
                }
            }
        }

        if has_audio && last_update.elapsed() >= Duration::from_millis(250) {
            let transcript = stream.poll(false)?;

            publish(transcript.text(), &mut last_text, &tx)?;

            last_update = Instant::now();
        }
    }
    drop(input);
    if has_audio && !tx.is_closed() {
        let tail = vad.finish();
        if !tail.is_empty() {
            stream.add_audio(&tail, sample_rate)?;
        }
        publish(stream.finalize()?.text(), &mut last_text, &tx)?;
    }
    Ok(())
}

fn publish(
    text: String,
    last_text: &mut String,
    tx: &mpsc::Sender<color_eyre::Result<String>>,
) -> color_eyre::Result<()> {
    if !text.trim().is_empty() && text != *last_text {
        tx.blocking_send(Ok(text.clone()))?;
        *last_text = text;
    }
    Ok(())
}
