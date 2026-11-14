use rig::{
    DynModel,
    catalog::Catalog,
    operation::Completion,
    providers::registry::{ConnectError, ConnectOptions},
};

pub async fn connect_at(
    provider: &str,
    credentials: Option<&str>,
    base_url: Option<&str>,
) -> Result<DynModel<Completion>, ConnectError> {
    let mut options = ConnectOptions::new();
    if let Some(credentials) = credentials {
        options = options.api_key(credentials);
    }
    if let Some(base_url) = base_url {
        let base_url = base_url.trim_end_matches('/');
        // Ollama's default OpenAI-compatible wire is served under /v1.
        let url = if provider.starts_with("ollama/")
            && !provider.starts_with("ollama/anthropic:")
            && !base_url.ends_with("/v1")
        {
            format!("{base_url}/v1")
        } else {
            base_url.to_owned()
        };
        options = options.base_url(url);
    }
    Catalog::builtin().connect_with(provider, options)
}
