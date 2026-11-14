use rig::tool::Tool;
use tokio::sync::mpsc;

pub struct SpeechTool(mpsc::Sender<String>);

impl Tool for SpeechTool {}
