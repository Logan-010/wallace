async fn connect(provider: &str, credentials: Option<&str>) -> Result<DynModel<Completion>, Error> {
    match credentials {
        Some(c) => Ok(registry::connect(provider, c)?),
        None => Ok(ProviderRef::from_str(provider)?.completion_model()?),
    }
}

pub async fn connect_at(
    provider: &str,
    credentials: Option<&str>,
    base_url: Option<&str>,
) -> Result<DynModel<Completion>, Error> {
    let Some(base_url) = base_url else {
        return connect(provider, credentials).await;
    };
    let reference = ProviderRef::from_str(provider)?;
    let key = credentials.unwrap_or("");
    let base_url = base_url.trim_end_matches('/');
    // The registry uses Ollama's OpenAI-compatible wire, served under /v1.
    let url = if provider.starts_with("ollama") && !base_url.ends_with("/v1") {
        format!("{base_url}/v1")
    } else {
        base_url.to_owned()
    };
    if let registry::ProviderConfig::OpenAi(config) = reference.config(key).with_base_url(url) {
        Ok(config.client().completion(reference.model()).erase())
    } else {
        connect(provider, credentials).await
    }
}
