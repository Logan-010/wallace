use tokio_rusqlite::Connection;

pub struct RemindersTool(Connection);

impl RemindersTool {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.clone())
    }
}
