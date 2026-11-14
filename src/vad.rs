use silero_vad_crs::SileroVad;
use std::collections::VecDeque;

const PRE_ROLL: usize = 5;
const HANGOVER: usize = 16;

pub enum Event {
    Audio(Vec<f32>),
    End,
}

pub struct Vad {
    model: SileroVad,
    pending: Vec<f32>,
    gate: Gate,
}

impl Vad {
    pub fn new(sample_rate: u32) -> color_eyre::Result<Self> {
        Ok(Self {
            model: SileroVad::with_sample_rate(sample_rate as usize)?,
            pending: Vec::new(),
            gate: Gate::default(),
        })
    }

    pub fn push(&mut self, samples: &[f32]) -> color_eyre::Result<Vec<Event>> {
        self.pending.extend_from_slice(samples);
        let size = self.model.source_window_samples();
        let consumed = self.pending.len() / size * size;
        let mut events = Vec::new();
        for frame in self.pending[..consumed].chunks_exact(size) {
            let probability = self.model.forward_chunk(frame)?;
            self.gate.push(frame, probability, &mut events);
        }
        self.pending.drain(..consumed);
        Ok(events)
    }

    pub fn finish(&mut self) -> Vec<f32> {
        if self.gate.active {
            std::mem::take(&mut self.pending)
        } else {
            Vec::new()
        }
    }
}

#[derive(Default)]
struct Gate {
    pre_roll: VecDeque<Vec<f32>>,
    active: bool,
    silence: usize,
}

impl Gate {
    fn push(&mut self, frame: &[f32], probability: f32, events: &mut Vec<Event>) {
        if !self.active {
            if probability < 0.5 {
                self.pre_roll.push_back(frame.to_vec());
                if self.pre_roll.len() > PRE_ROLL {
                    self.pre_roll.pop_front();
                }
                return;
            }
            self.active = true;
            events.extend(self.pre_roll.drain(..).map(Event::Audio));
        }

        events.push(Event::Audio(frame.to_vec()));
        self.silence = if probability < 0.35 {
            self.silence + 1
        } else {
            0
        };
        if self.silence >= HANGOVER {
            self.active = false;
            self.silence = 0;
            events.push(Event::End);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_keeps_pre_roll_and_ends_only_after_hangover() {
        let mut gate = Gate::default();
        let mut events = Vec::new();
        for _ in 0..20 {
            gate.push(&[0.0], 0.0, &mut events);
        }
        assert!(events.is_empty());
        gate.push(&[1.0], 0.9, &mut events);
        assert_eq!(events.len(), PRE_ROLL + 1);
        for _ in 0..HANGOVER - 1 {
            gate.push(&[0.0], 0.0, &mut events);
        }
        assert!(gate.active);
        gate.push(&[0.0], 0.4, &mut events);
        assert_eq!(gate.silence, 0);
        for _ in 0..HANGOVER {
            gate.push(&[0.0], 0.0, &mut events);
        }
        assert!(matches!(events.last(), Some(Event::End)));
        assert!(!gate.active);
        events.clear();
        gate.push(&[0.0], 0.0, &mut events);
        assert!(events.is_empty());
    }

    #[test]
    fn accepts_split_microphone_buffers_at_common_rates() {
        for rate in [16_000, 44_100, 48_000] {
            let mut vad = Vad::new(rate).unwrap();
            for _ in 0..20 {
                assert!(vad.push(&[0.0; 127]).unwrap().is_empty());
            }
            assert!(vad.pending.len() < vad.model.source_window_samples());
            assert!(vad.finish().is_empty());
        }
    }

    #[test]
    fn finish_preserves_pending_audio_during_speech() {
        let mut vad = Vad::new(16_000).unwrap();
        vad.gate.active = true;
        vad.pending = vec![0.25; 100];
        assert_eq!(vad.finish(), vec![0.25; 100]);
        assert!(vad.finish().is_empty());
    }
}
