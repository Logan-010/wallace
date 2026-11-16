use tokio_rusqlite::Connection;

pub struct NotesTool(Connection);

impl NotesTool {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.clone())
    }
}
