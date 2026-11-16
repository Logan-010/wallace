use tokio_rusqlite::{Connection, OptionalExtension};

pub async fn migrate(conn: &Connection) -> color_eyre::Result<()> {
    conn.call(|db| {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv_store (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );",
        )?;

        Ok(())
    })
    .await?;

    Ok(())
}

pub async fn get(conn: &Connection, key: String) -> tokio_rusqlite::Result<Option<String>> {
    conn.call(move |db| {
        let value = db
            .query_row("SELECT value FROM kv_store WHERE key = ?", [&key], |row| {
                row.get(0)
            })
            .optional()?;

        Ok(value)
    })
    .await
}

pub async fn put(conn: &Connection, key: String, value: String) -> tokio_rusqlite::Result<()> {
    conn.call(move |db| {
        db.execute(
            "INSERT INTO kv_store (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [&key, &value],
        )?;

        Ok(())
    })
    .await
}

pub async fn remove(conn: &Connection, key: String) -> tokio_rusqlite::Result<()> {
    conn.call(move |db| {
        db.execute("DELETE FROM kv_store WHERE key = ?", [&key])?;

        Ok(())
    })
    .await
}
