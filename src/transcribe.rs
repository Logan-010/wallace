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
    sync::RwLock,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task};

static LISTENING: RwLock<u64> = RwLock::new(0);

pub struct Pause<'a>(&'a RwLock<u64>);

impl<'a> Pause<'a> {
    fn new(state: &'a RwLock<u64>) -> Self {
        let mut epoch = state.write().unwrap();
        *epoch = epoch.wrapping_add(1);
        Self(state)
    }
}

impl Drop for Pause<'_> {
    fn drop(&mut self) {
        let mut epoch = self.0.write().unwrap();
        *epoch = epoch.wrapping_add(1);
    }
}

pub fn pause() -> Pause<'static> {
    Pause::new(&LISTENING)
}

fn listening_epoch(state: &RwLock<u64>) -> Option<u64> {
    let epoch = *state.try_read().ok()?;
    (epoch % 2 == 0).then_some(epoch)
}

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
        std::sync::mpsc::sync_channel::<color_eyre::Result<(u64, u64, Vec<f32>)>>(32);
    let error_tx = audio_tx.clone();

    let input = device.build_input_stream(
        config,
        move |data: &[f32], _| {
            let Some(epoch) = listening_epoch(&LISTENING) else {
                return;
            };
            let Ok(generation) = PLAYBACK.try_read() else {
                return;
            };
            let mono: Vec<f32> = data
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();

            let _ = audio_tx.try_send(Ok((epoch, *generation, mono)));
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
    let mut epoch = 0;
    while !TOKEN.is_cancelled() && !tx.is_closed() {
        let Some(current_epoch) = listening_epoch(&LISTENING) else {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        let Some(playback) = playback_generation(&PLAYBACK) else {
            std::thread::sleep(Duration::from_millis(20));
            continue;
        };
        if generation != playback || epoch != current_epoch {
            vad = Vad::new(sample_rate)?;
            stream.restart()?;
            has_audio = false;
            last_update = Instant::now();
            generation = playback;
            epoch = current_epoch;
            tracing::debug!(generation, epoch, "STT listening resumed");
        }
        let (captured_epoch, captured_generation, samples) =
            match audio_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(samples) => samples?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(error) => return Err(error.into()),
            };
        if captured_generation != generation
            || captured_epoch != epoch
            || listening_epoch(&LISTENING) != Some(epoch)
        {
            continue;
        }
        for event in vad.push(&samples)? {
            match event {
                Event::Audio(audio) => {
                    stream.add_audio(&audio, sample_rate)?;
                    has_audio = true;
                }
                Event::End => {
                    let text = stream.finalize()?.text();
                    publish_if_listening(text, &tx, &LISTENING, epoch)?;
                    stream = ts.create_stream()?;
                    has_audio = false;
                }
            }
        }

        if has_audio
            && listening_epoch(&LISTENING) == Some(epoch)
            && last_update.elapsed() >= Duration::from_millis(250)
        {
            // Keep inference incremental, but only publish finalized utterances.
            stream.poll(false)?;

            last_update = Instant::now();
        }
    }
    drop(input);
    Ok(())
}

fn publish_if_listening(
    text: String,
    tx: &mpsc::Sender<color_eyre::Result<String>>,
    state: &RwLock<u64>,
    epoch: u64,
) -> color_eyre::Result<()> {
    // Serialize publishing with pause transitions, but never block on a full queue.
    let current = state.read().unwrap();
    if *current != epoch || epoch % 2 != 0 {
        return Ok(());
    }
    if !text.trim().is_empty() {
        tracing::debug!("Recognized speech: {text}");
        match tx.try_send(Ok(text)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::debug!("Discarding speech while the query queue is full");
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn playback_generation(playback: &std::sync::RwLock<u64>) -> Option<u64> {
    // Release the read lock before waiting for audio or running inference.
    // Playback must not have to wait for the transcription worker to finish.
    playback.try_read().ok().map(|generation| *generation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

    #[test]
    fn playback_snapshot_does_not_hold_the_read_lock() {
        let playback = RwLock::new(0);
        let generation = playback_generation(&playback).unwrap();
        let mut active = playback.try_write().unwrap();
        *active += 1;
        assert_eq!(generation, 0);
        assert!(playback_generation(&playback).is_none());
        drop(active);
        assert_eq!(playback_generation(&playback), Some(1));
    }

    #[tokio::test]
    async fn publishes_multiple_utterances() {
        let (tx, mut rx) = mpsc::channel(2);
        task::spawn_blocking(move || {
            let state = RwLock::new(0);
            publish_if_listening("First query".to_owned(), &tx, &state, 0).unwrap();
            publish_if_listening(String::new(), &tx, &state, 0).unwrap();
            publish_if_listening("Second query".to_owned(), &tx, &state, 0).unwrap();
        })
        .await
        .unwrap();
        assert_eq!(rx.recv().await.unwrap().unwrap(), "First query");
        assert_eq!(rx.recv().await.unwrap().unwrap(), "Second query");
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn pause_discards_in_flight_speech_and_resumes_with_a_new_epoch() {
        let state = RwLock::new(0);
        let (tx, mut rx) = mpsc::channel(1);
        let pause = Pause::new(&state);
        assert_eq!(listening_epoch(&state), None);
        publish_if_listening("During run".to_owned(), &tx, &state, 0).unwrap();
        drop(pause);
        assert_eq!(listening_epoch(&state), Some(2));
        publish_if_listening("Stale audio".to_owned(), &tx, &state, 0).unwrap();
        assert!(rx.try_recv().is_err());
        publish_if_listening("New query".to_owned(), &tx, &state, 2).unwrap();
        assert_eq!(rx.recv().await.unwrap().unwrap(), "New query");
    }
}
