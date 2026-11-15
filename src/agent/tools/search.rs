use rig::tool::{Tool, ToolContext};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{env, time::Duration};

#[derive(Clone)]
pub struct SearchTool {
    client: reqwest::Client,
    api_key: String,
}

impl SearchTool {
    pub fn new(api_key: Option<String>) -> color_eyre::Result<Self> {
        let api_key = resolve_api_key(api_key, || env::var("BRAVE_API_KEY").ok())?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()?;

        Ok(Self { client, api_key })
    }
}

fn resolve_api_key(
    api_key: Option<String>,
    from_env: impl FnOnce() -> Option<String>,
) -> color_eyre::Result<String> {
    api_key
        .filter(|key| !key.trim().is_empty())
        .or_else(|| from_env().filter(|key| !key.trim().is_empty()))
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Brave API key is required: pass an API key or set BRAVE_API_KEY."
            )
        })
}

#[derive(Deserialize)]
pub struct SearchArgs {
    pub query: String,
}

impl Tool for SearchTool {
    const NAME: &'static str = "web_search";
    type Args = SearchArgs;
    type Output = Value;
    type Error = reqwest::Error;

    fn description(&self) -> String {
        "Search the live web for recent information. Returns source titles, URLs, and snippets."
            .to_owned()
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search engine query"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        tracing::info!("Searching for: {}", args.query);

        let data: Value = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[("q", args.query.as_str()), ("count", "5")])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let results: Vec<Value> = data["web"]["results"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                json!({
                    "title": item["title"],
                    "url": item["url"],
                    "snippet": item["description"],
                })
            })
            .collect();

        Ok(json!({ "results": results }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_uses_explicit_key() {
        let tool = SearchTool::new(Some("explicit-key".to_owned())).unwrap();
        assert_eq!(tool.api_key, "explicit-key");
    }

    #[test]
    fn explicit_key_takes_precedence() {
        let key = resolve_api_key(Some("explicit-key".to_owned()), || {
            panic!("environment should not be read when a key is provided")
        })
        .unwrap();
        assert_eq!(key, "explicit-key");
    }

    #[test]
    fn falls_back_to_environment() {
        for key in [None, Some("  ".to_owned())] {
            assert_eq!(
                resolve_api_key(key, || Some("env-key".to_owned())).unwrap(),
                "env-key"
            );
        }
    }

    #[test]
    fn rejects_missing_or_empty_keys() {
        for key in [None, Some("".to_owned()), Some("  ".to_owned())] {
            let error = resolve_api_key(None, || key).unwrap_err();
            assert!(error.to_string().contains("BRAVE_API_KEY"));
        }
    }

    #[test]
    fn requires_query_argument() {
        assert!(serde_json::from_value::<SearchArgs>(json!({})).is_err());
        assert!(serde_json::from_value::<SearchArgs>(json!({ "query": 42 })).is_err());
    }
}
