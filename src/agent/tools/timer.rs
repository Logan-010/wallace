use tokio::sync::mpsc;

pub struct TimersTool(mpsc::Sender<String>);

impl TimersTool {
    pub fn new() -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(100);

        (Self(tx), rx)
    }
}
