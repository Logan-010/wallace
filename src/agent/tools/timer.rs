use rig::tool::{Tool, ToolContext};
use serde::Deserialize;
use std::{error::Error, fmt, time::Duration};
use tokio::{sync::mpsc, time::Instant};

pub struct TimersTool(mpsc::Sender<String>);

impl TimersTool {
    pub fn new() -> (Self, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel(100);

        (Self(tx), rx)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerArgs {
    pub name: String,
    pub duration_seconds: u64,
}

#[derive(Debug)]
pub struct TimerError(String);

impl fmt::Display for TimerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for TimerError {}

impl Tool for TimersTool {
    const NAME: &'static str = "set_timer";
    type Args = TimerArgs;
    type Output = String;
    type Error = TimerError;

    fn description(&self) -> String {
        "Set a named timer for a positive duration in seconds. Returns immediately and announces the name when it expires. Timers only last while the application is running; each call creates an independent timer."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The name to announce when the timer expires",
                    "minLength": 1
                },
                "duration_seconds": {
                    "type": "integer",
                    "description": "How long to wait, in seconds",
                    "minimum": 1
                }
            },
            "required": ["name", "duration_seconds"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let name = args.name.trim().to_owned();
        if name.is_empty() {
            return Err(TimerError("Timer name must not be empty.".to_owned()));
        }
        if args.duration_seconds == 0 {
            return Err(TimerError("Timer duration must be positive.".to_owned()));
        }
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(args.duration_seconds))
            .ok_or_else(|| TimerError("Timer duration is too large.".to_owned()))?;
        if self.0.is_closed() {
            return Err(TimerError(
                "Timer notification channel is closed.".to_owned(),
            ));
        }

        let confirmation = format!("Timer '{name}' set for {} seconds.", args.duration_seconds);
        let tx = self.0.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    if tx.send(name).await.is_err() {
                        tracing::warn!("Failed to send timer notification");
                    }
                }
                _ = tx.closed() => {}
            }
        });

        Ok(confirmation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn set_timer(tool: &TimersTool, name: &str, seconds: u64) -> Result<String, TimerError> {
        tool.call(
            &mut ToolContext::default(),
            TimerArgs {
                name: name.to_owned(),
                duration_seconds: seconds,
            },
        )
        .await
    }

    #[tokio::test(start_paused = true)]
    async fn announces_name_only_after_duration() {
        let (tool, mut rx) = TimersTool::new();
        let confirmation = set_timer(&tool, " tea ", 60).await.unwrap();
        assert!(confirmation.contains("tea"));
        assert!(confirmation.contains("60"));
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(rx.try_recv().is_err());
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(rx.recv().await.unwrap(), "tea");
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn timers_run_independently_and_survive_tool_drop() {
        let (tool, mut rx) = TimersTool::new();
        set_timer(&tool, "long", 120).await.unwrap();
        set_timer(&tool, "short", 30).await.unwrap();
        drop(tool);

        assert_eq!(rx.recv().await.unwrap(), "short");
        assert_eq!(rx.recv().await.unwrap(), "long");
        assert_eq!(rx.recv().await, None);
    }

    #[tokio::test]
    async fn rejects_invalid_timers() {
        let (tool, mut rx) = TimersTool::new();
        assert!(set_timer(&tool, "  ", 1).await.is_err());
        assert!(set_timer(&tool, "tea", 0).await.is_err());
        assert!(set_timer(&tool, "tea", u64::MAX).await.is_err());
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn rejects_closed_notification_channel() {
        let (tool, rx) = TimersTool::new();
        drop(rx);
        assert!(set_timer(&tool, "tea", 60).await.is_err());
    }

    #[test]
    fn validates_arguments() {
        for args in [
            serde_json::json!({"name": "tea"}),
            serde_json::json!({"duration_seconds": 60}),
            serde_json::json!({"name": "tea", "duration_seconds": -1}),
            serde_json::json!({"name": "tea", "duration_seconds": 1.5}),
            serde_json::json!({"name": "tea", "duration_seconds": 60, "extra": true}),
        ] {
            assert!(serde_json::from_value::<TimerArgs>(args).is_err());
        }
    }
}
