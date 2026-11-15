mod task;
pub use task::task;

mod tool;
pub use tool::SpeechTool;

pub const VOICE: &str = "piper_en_US-ryan-low";

pub static PLAYBACK: std::sync::RwLock<u64> = std::sync::RwLock::new(0);
