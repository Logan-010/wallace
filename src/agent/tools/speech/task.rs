use super::VOICE;
use crate::{TOKEN, cli::Cli};
use color_eyre::eyre::{ContextCompat, bail, ensure};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use moonshine_rs::{SynthesizedAudio, TtsOptions, TtsSynthesizer};
use std::{
    env,
    path::PathBuf,
    sync::mpsc as sync_mpsc,
    thread,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task};

pub async fn task(
    cli: &Cli,
) -> color_eyre::Result<(mpsc::Sender<String>, mpsc::Receiver<color_eyre::Result<()>>)> {
    let dir = match cli.dir.as_ref() {
        Some(d) => d.to_path_buf(),
        None => env::home_dir()
            .context("Expected home dir")?
            .join(".wallace")
            .join("models"),
    };
    let (tx, rx) = mpsc::channel(100);
    let (result_tx, result_rx) = mpsc::channel(100);

    task::spawn(async move {
        let worker_tx = result_tx.clone();
        let result = task::spawn_blocking(move || blocking_task(dir, rx, worker_tx)).await;
        let error = match result {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error,
            Err(error) => error.into(),
        };
        let _ = result_tx.send(Err(error)).await;
    });

    Ok((tx, result_rx))
}

fn blocking_task(
    dir: PathBuf,
    mut rx: mpsc::Receiver<String>,
    tx: mpsc::Sender<color_eyre::Result<()>>,
) -> color_eyre::Result<()> {
    let options = TtsOptions::new().with_voice(VOICE);
    let synthesizer = TtsSynthesizer::from_files("en", dir, Some(&options))?;
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .context("No audio output device found")?;
    let supported = device.default_output_config()?;

    while !TOKEN.is_cancelled() && !tx.is_closed() {
        let text = match rx.try_recv() {
            Ok(text) => text,
            Err(mpsc::error::TryRecvError::Empty) => {
                thread::sleep(Duration::from_millis(20));
                continue;
            }
            Err(mpsc::error::TryRecvError::Disconnected) => break,
        };
        if !text.trim().is_empty() {
            let started = Instant::now();
            tracing::debug!("Synthesizing TTS: {text}");
            let audio = synthesizer.synthesize(&text, None)?;
            tracing::info!(
                synthesis_ms = started.elapsed().as_millis(),
                audio_seconds = audio.pcm.len() as f64 / audio.sample_rate as f64,
                "TTS synthesized"
            );
            if TOKEN.is_cancelled() || tx.is_closed() {
                break;
            }
            let config = supported.config();
            let mut playback = super::PLAYBACK.write().unwrap();
            *playback = playback.wrapping_add(1);
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => output_stream::<f32>(&device, config, audio),
                cpal::SampleFormat::F64 => output_stream::<f64>(&device, config, audio),
                cpal::SampleFormat::I8 => output_stream::<i8>(&device, config, audio),
                cpal::SampleFormat::I16 => output_stream::<i16>(&device, config, audio),
                cpal::SampleFormat::I32 => output_stream::<i32>(&device, config, audio),
                cpal::SampleFormat::I64 => output_stream::<i64>(&device, config, audio),
                cpal::SampleFormat::U8 => output_stream::<u8>(&device, config, audio),
                cpal::SampleFormat::U16 => output_stream::<u16>(&device, config, audio),
                cpal::SampleFormat::U32 => output_stream::<u32>(&device, config, audio),
                cpal::SampleFormat::U64 => output_stream::<u64>(&device, config, audio),
                format => bail!("Unsupported output sample format: {format}"),
            }?;
            stream.0.play()?;
            let started = Instant::now();
            tracing::info!("TTS playback started");
            loop {
                if TOKEN.is_cancelled() || tx.is_closed() {
                    return Ok(());
                }
                match stream.1.recv_timeout(Duration::from_millis(20)) {
                    Ok(result) => {
                        let deadline = result?;
                        while Instant::now() < deadline {
                            if TOKEN.is_cancelled() || tx.is_closed() {
                                return Ok(());
                            }
                            thread::sleep(
                                deadline
                                    .saturating_duration_since(Instant::now())
                                    .min(Duration::from_millis(20)),
                            );
                        }
                        break;
                    }
                    Err(sync_mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            tracing::info!(
                playback_ms = started.elapsed().as_millis(),
                "TTS playback finished"
            );
        }
        let mut result = Ok(());
        loop {
            if TOKEN.is_cancelled() || tx.is_closed() {
                return Ok(());
            }
            match tx.try_send(result) {
                Ok(()) => break,
                Err(mpsc::error::TrySendError::Full(value)) => {
                    result = value;
                    thread::sleep(Duration::from_millis(20));
                }
                Err(mpsc::error::TrySendError::Closed(_)) => return Ok(()),
            }
        }
    }
    Ok(())
}

fn output_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    audio: SynthesizedAudio,
) -> color_eyre::Result<(
    cpal::Stream,
    sync_mpsc::Receiver<color_eyre::Result<Instant>>,
)>
where
    T: cpal::SizedSample + cpal::FromSample<f32>,
{
    ensure!(audio.sample_rate > 0, "TTS returned an invalid sample rate");
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate;
    ensure!(
        channels > 0 && sample_rate > 0,
        "Invalid output configuration"
    );
    let mut samples = Samples {
        pcm: audio.pcm,
        position: 0.0,
        step: audio.sample_rate as f64 / sample_rate as f64,
    };
    let (tx, rx) = sync_mpsc::sync_channel(1);
    let error_tx = tx.clone();
    let mut finished = false;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], info| {
            for frame in data.chunks_mut(channels) {
                let sample = T::from_sample(samples.next().unwrap_or(0.0));
                frame.fill(sample);
            }
            if !finished && samples.position >= samples.pcm.len() as f64 {
                let timestamp = info.timestamp();
                let latency = timestamp.playback.duration_since(timestamp.callback);
                let duration = Duration::from_secs_f64(
                    data.len() as f64 / channels as f64 / sample_rate as f64,
                );
                let _ = tx.try_send(Ok(Instant::now() + latency + duration));
                finished = true;
            }
        },
        move |error| {
            let _ = error_tx.try_send(Err(error.into()));
        },
        None,
    )?;
    Ok((stream, rx))
}

struct Samples {
    pcm: Vec<f32>,
    position: f64,
    step: f64,
}

impl Iterator for Samples {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.position as usize;
        let first = *self.pcm.get(index)?;
        let second = self.pcm.get(index + 1).copied().unwrap_or(first);
        let fraction = (self.position - index as f64) as f32;
        self.position += self.step;
        Some((first + (second - first) * fraction).clamp(-1.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_samples_at_native_rate() {
        let samples = Samples {
            pcm: vec![-1.0, 0.0, 1.0],
            position: 0.0,
            step: 1.0,
        };
        assert_eq!(samples.collect::<Vec<_>>(), vec![-1.0, 0.0, 1.0]);
    }

    #[test]
    fn interpolates_for_higher_output_rate() {
        let samples = Samples {
            pcm: vec![0.0, 1.0],
            position: 0.0,
            step: 0.5,
        };
        assert_eq!(samples.collect::<Vec<_>>(), vec![0.0, 0.5, 1.0, 1.0]);
    }

    #[test]
    fn resamples_for_lower_output_rate() {
        let samples = Samples {
            pcm: vec![0.0, 0.5, 1.0, 0.5],
            position: 0.0,
            step: 2.0,
        };
        assert_eq!(samples.collect::<Vec<_>>(), vec![0.0, 1.0]);
    }

    #[test]
    fn handles_empty_audio() {
        let mut samples = Samples {
            pcm: Vec::new(),
            position: 0.0,
            step: 1.0,
        };
        assert_eq!(samples.next(), None);
    }
}
