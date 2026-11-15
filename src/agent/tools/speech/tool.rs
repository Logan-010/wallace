use rig::tool::{Tool, ToolContext};
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct SpeechTool(mpsc::Sender<String>);

impl SpeechTool {
    pub fn new(sender: mpsc::Sender<String>) -> Self {
        Self(sender)
    }
}

#[derive(Deserialize)]
pub struct SpeechArgs {
    pub text: String,
}

impl Tool for SpeechTool {
    const NAME: &'static str = "speech";
    type Args = SpeechArgs;
    type Output = &'static str;
    type Error = mpsc::error::SendError<String>;

    fn description(&self) -> String {
        "Queue text to be spoken aloud to the user. Returns once queued, not when playback finishes."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to speak aloud."
                }
            },
            "required": ["text"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        tracing::info!("Sending TTS: {}", args.text);
        self.0.send(args.text).await?;
        Ok("Speech queued.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forwards_text_to_sender() {
        let (tx, mut rx) = mpsc::channel(1);
        let tool = SpeechTool::new(tx);
        let args = serde_json::from_value(json!({ "text": "Hello, world!" })).unwrap();

        let result = tool.call(&mut ToolContext::default(), args).await.unwrap();

        assert_eq!(result, "Speech queued.");
        assert_eq!(rx.recv().await.as_deref(), Some("Hello, world!"));
    }

    #[tokio::test]
    async fn reports_closed_sender() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let tool = SpeechTool::new(tx);
        let args = SpeechArgs {
            text: "Hello!".to_owned(),
        };

        let error = tool
            .call(&mut ToolContext::default(), args)
            .await
            .unwrap_err();

        assert_eq!(error.0, "Hello!");
    }

    #[test]
    fn requires_text_argument() {
        assert!(serde_json::from_value::<SpeechArgs>(json!({})).is_err());
        assert!(serde_json::from_value::<SpeechArgs>(json!({ "text": 42 })).is_err());
    }
}
