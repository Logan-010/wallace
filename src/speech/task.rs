use tokio::sync::mpsc;

pub async fn task() -> color_eyre::Result<mpsc::Sender<String>> {
    let (tx, rx) = mpsc::channel(100);

    Ok(tx)
}
