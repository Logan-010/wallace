use tokio::sync::mpsc;
use tokio_rusqlite::Connection;

pub struct RemindersTool {
    db: Connection,
    tx: mpsc::Sender<String>,
}

impl RemindersTool {
    pub fn new(conn: &Connection) -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(100);
        (
            Self {
                db: conn.clone(),
                tx,
            },
            rx,
        )
    }
}
