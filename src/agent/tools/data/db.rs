use tokio_rusqlite::Connection;

pub async fn migrate(conn: &Connection) -> color_eyre::Result<()> {
    conn.call(|db| {
        let tx = db.transaction()?;

        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv_store (
                key TEXT NOT NULL UNIQUE PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;

        tx.commit()?;

        Ok(())
    })
    .await?;

    Ok(())
}

pub async fn get(conn: &Connection, key: String) -> color_eyre::Result<String> {
    todo!()
}

pub async fn put(conn: &Connection, key: String, value: String) -> color_eyre::Result<()> {
    todo!()
}

pub async fn remove(conn: &Connection, key: String) -> color_eyre::Result<()> {
    todo!()
}
