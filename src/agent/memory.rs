use rig::{
    conversation::Conversation,
    memory::{
        AsyncStore, MemoryStore, load_async,
        recent::{LastMessages, view},
        save_async,
    },
    sqlite::SqliteSessionStore,
};
use std::io;

pub const LOG: &str = "agent";

pub async fn prune(store: &SqliteSessionStore, limit: usize) -> color_eyre::Result<()> {
    let bytes = match store.read_log_async(LOG).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let len = bytes.len() as u64;
    let conversation = load_async(store, LOG).await?;
    let retained = limit.saturating_mul(2);
    if conversation.messages().len() <= retained {
        return Ok(());
    }

    let mut recent = Conversation::new();
    for message in view(conversation.messages(), &LastMessages(retained)) {
        recent.push(message.as_ref().clone());
    }
    let replacement = MemoryStore::default();
    save_async(&replacement, LOG, &mut recent).await?;
    let bytes = replacement.read_log_async(LOG).await?;
    store.replace_log_async(LOG, len, &bytes).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::Message;
    use tokio_rusqlite::Connection;

    async fn store(count: usize) -> color_eyre::Result<SqliteSessionStore> {
        let store = SqliteSessionStore::open(Connection::open_in_memory().await?).await?;
        let mut conversation = Conversation::new();
        for index in 0..count {
            conversation.push(Message::user(format!("message-{index:03}")));
        }
        save_async(&store, LOG, &mut conversation).await?;
        Ok(store)
    }

    #[tokio::test]
    async fn missing_history_is_unchanged() -> color_eyre::Result<()> {
        let store = SqliteSessionStore::open(Connection::open_in_memory().await?).await?;
        prune(&store, 10).await?;
        assert!(load_async(&store, LOG).await?.messages().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn history_within_retention_is_unchanged() -> color_eyre::Result<()> {
        for count in [1, 19, 20] {
            let store = store(count).await?;
            let before = store.read_log_async(LOG).await?;
            prune(&store, 10).await?;
            assert_eq!(store.read_log_async(LOG).await?, before);
        }
        Ok(())
    }

    #[tokio::test]
    async fn old_messages_are_deleted_and_new_messages_can_be_saved() -> color_eyre::Result<()> {
        let store = store(25).await?;
        prune(&store, 10).await?;
        let mut conversation = load_async(&store, LOG).await?;
        assert_eq!(conversation.messages().len(), 20);
        assert_eq!(
            conversation.messages()[0].as_ref(),
            &Message::user("message-005")
        );
        let history = view(conversation.messages(), &LastMessages(10));
        assert_eq!(history.len(), 10);
        assert_eq!(history[0].as_ref(), &Message::user("message-015"));
        let bytes = String::from_utf8(store.read_log_async(LOG).await?)?;
        assert!(!bytes.contains("message-000"));
        assert!(!bytes.contains("message-004"));

        conversation.push(Message::assistant("new response"));
        save_async(&store, LOG, &mut conversation).await?;
        prune(&store, 10).await?;
        let conversation = load_async(&store, LOG).await?;
        assert_eq!(conversation.messages().len(), 20);
        assert_eq!(
            conversation.messages().last().unwrap().as_ref(),
            &Message::assistant("new response")
        );
        Ok(())
    }

    #[tokio::test]
    async fn custom_limit_is_applied() -> color_eyre::Result<()> {
        let store = store(10).await?;
        prune(&store, 1).await?;
        let conversation = load_async(&store, LOG).await?;
        assert_eq!(conversation.messages().len(), 2);
        assert_eq!(
            conversation.messages()[0].as_ref(),
            &Message::user("message-008")
        );
        Ok(())
    }

    #[tokio::test]
    async fn pruning_does_not_leave_orphaned_tool_results() -> color_eyre::Result<()> {
        use rig::message::{
            AssistantContent, AssistantMessage, CallId, ToolCall, ToolFunction, ToolName,
            ToolResultContent, UserContent,
        };

        let store = store(1).await?;
        let mut conversation = load_async(&store, LOG).await?;
        let name = ToolName::new("test")?;
        let call = ToolCall::new(
            CallId::from_wire("call-1"),
            ToolFunction::new(name.clone(), serde_json::json!({})),
        );
        conversation.push(Message::Assistant(AssistantMessage::new(vec![
            AssistantContent::ToolCall(call),
        ])));
        conversation.push(Message::User {
            content: vec![UserContent::tool_result(
                CallId::from_wire("call-1"),
                name,
                vec![ToolResultContent::text("result")],
            )],
        });
        conversation.push(Message::assistant("done"));
        save_async(&store, LOG, &mut conversation).await?;

        prune(&store, 1).await?;
        let conversation = load_async(&store, LOG).await?;
        assert_eq!(
            conversation.messages(),
            &[Message::assistant("done").into()]
        );
        Ok(())
    }
}
