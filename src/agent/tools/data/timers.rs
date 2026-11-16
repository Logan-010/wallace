use tokio_rusqlite::Connection;

pub struct TimersTool(Connection);

impl TimersTool {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.clone())
    }
}
