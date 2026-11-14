mod task;
pub use task::task;

mod tool;
pub use tool::SpeechTool;

pub(crate) const VOICE: &str = "piper_en_US-ryan-low";

// Playback holds the write lock; microphone capture and STT hold read locks.
// The generation invalidates audio and partial transcripts from before playback.
pub(crate) static PLAYBACK: std::sync::RwLock<u64> = std::sync::RwLock::new(0);
