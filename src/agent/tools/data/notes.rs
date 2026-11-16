use super::db;
use rig::tool::{Tool, ToolContext};
use serde::Deserialize;
use std::{error::Error, fmt};
use tokio_rusqlite::Connection;

pub struct NotesTool(Connection);

impl NotesTool {
    pub fn new(conn: &Connection) -> Self {
        Self(conn.clone())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum NotesArgs {
    Get { name: String },
    Put { name: String, content: String },
    Delete { name: String },
}

#[derive(Debug)]
pub struct NotesError(String);

impl fmt::Display for NotesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for NotesError {}

impl From<tokio_rusqlite::Error> for NotesError {
    fn from(error: tokio_rusqlite::Error) -> Self {
        Self(format!("Notes database error: {error}"))
    }
}

impl Tool for NotesTool {
    const NAME: &'static str = "notes";
    type Args = NotesArgs;
    type Output = String;
    type Error = NotesError;

    fn description(&self) -> String {
        "Store and retrieve persistent named notes. Use get to view a note, put to create or completely replace its content, and delete to remove it. All actions require a name; put also requires content. Names are case-sensitive and trimmed of surrounding whitespace. Deleting a missing note succeeds."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "put", "delete"]
                },
                "name": {
                    "type": "string",
                    "minLength": 1,
                    "description": "The note name, without the notes/ storage prefix"
                },
                "content": {
                    "type": "string",
                    "description": "Required only for put: the complete note content"
                }
            },
            "required": ["action", "name"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        _context: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let name = match &args {
            NotesArgs::Get { name } | NotesArgs::Put { name, .. } | NotesArgs::Delete { name } => {
                name.trim().to_owned()
            }
        };
        if name.is_empty() {
            return Err(NotesError("Note name must not be empty.".to_owned()));
        }
        let key = format!("notes/{name}");
        match args {
            NotesArgs::Get { .. } => db::get(&self.0, key)
                .await?
                .ok_or_else(|| NotesError(format!("No note named '{name}'."))),
            NotesArgs::Put { content, .. } => {
                db::put(&self.0, key, content).await?;
                Ok(format!("Note '{name}' saved."))
            }
            NotesArgs::Delete { .. } => {
                db::remove(&self.0, key).await?;
                Ok(format!("Note '{name}' deleted (if it existed)."))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn setup() -> (Connection, NotesTool) {
        let conn = Connection::open_in_memory().await.unwrap();
        db::migrate(&conn).await.unwrap();
        let tool = NotesTool::new(&conn);
        (conn, tool)
    }

    async fn call(tool: &NotesTool, args: serde_json::Value) -> Result<String, NotesError> {
        tool.call(
            &mut ToolContext::default(),
            serde_json::from_value(args).unwrap(),
        )
        .await
    }

    #[tokio::test]
    async fn creates_reads_updates_and_deletes_named_notes() {
        let (conn, tool) = setup().await;
        call(
            &tool,
            json!({"action": "put", "name": "shopping", "content": "milk"}),
        )
        .await
        .unwrap();
        assert_eq!(
            db::get(&conn, "notes/shopping".into())
                .await
                .unwrap()
                .as_deref(),
            Some("milk")
        );
        assert_eq!(
            call(&tool, json!({"action": "get", "name": "shopping"}))
                .await
                .unwrap(),
            "milk"
        );

        call(
            &tool,
            json!({"action": "put", "name": "shopping", "content": "bread"}),
        )
        .await
        .unwrap();
        assert_eq!(
            call(&tool, json!({"action": "get", "name": "shopping"}))
                .await
                .unwrap(),
            "bread"
        );

        call(&tool, json!({"action": "delete", "name": "shopping"}))
            .await
            .unwrap();
        assert!(
            db::get(&conn, "notes/shopping".into())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            call(&tool, json!({"action": "get", "name": "shopping"}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn notes_are_available_to_a_new_tool_instance() {
        let (conn, tool) = setup().await;
        call(
            &tool,
            json!({"action": "put", "name": "remember", "content": "saved"}),
        )
        .await
        .unwrap();
        drop(tool);
        let tool = NotesTool::new(&conn);
        assert_eq!(
            call(&tool, json!({"action": "get", "name": "remember"}))
                .await
                .unwrap(),
            "saved"
        );
    }

    #[tokio::test]
    async fn preserves_content_exactly_including_empty_notes() {
        let (_, tool) = setup().await;
        for content in ["  First line\nSecond line 📝\n", ""] {
            call(
                &tool,
                json!({"action": "put", "name": "日記", "content": content}),
            )
            .await
            .unwrap();
            assert_eq!(
                call(&tool, json!({"action": "get", "name": "日記"}))
                    .await
                    .unwrap(),
                content
            );
        }
    }

    #[tokio::test]
    async fn trims_names_and_rejects_blank_names_for_all_actions() {
        let (conn, tool) = setup().await;
        call(
            &tool,
            json!({"action": "put", "name": "  plans  ", "content": "test"}),
        )
        .await
        .unwrap();
        assert_eq!(
            call(&tool, json!({"action": "get", "name": "plans"}))
                .await
                .unwrap(),
            "test"
        );
        for args in [
            json!({"action": "get", "name": ""}),
            json!({"action": "put", "name": " \n\t", "content": "test"}),
            json!({"action": "delete", "name": " "}),
        ] {
            assert!(call(&tool, args).await.is_err());
        }
        assert!(db::get(&conn, "notes/".into()).await.unwrap().is_none());
        call(&tool, json!({"action": "delete", "name": " plans "}))
            .await
            .unwrap();
        assert!(
            db::get(&conn, "notes/plans".into())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn missing_notes_return_an_error_on_get_and_delete_is_idempotent() {
        let (_, tool) = setup().await;
        let error = call(&tool, json!({"action": "get", "name": "missing"}))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("No note named 'missing'"));
        for _ in 0..2 {
            call(&tool, json!({"action": "delete", "name": "missing"}))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn operations_do_not_modify_other_notes_or_non_note_keys() {
        let (conn, tool) = setup().await;
        db::put(&conn, "reminders".into(), "keep".into())
            .await
            .unwrap();
        for name in [
            "reminders",
            "other",
            "../reminders",
            "Robert'); DROP TABLE kv_store;--",
        ] {
            call(
                &tool,
                json!({"action": "put", "name": name, "content": "note"}),
            )
            .await
            .unwrap();
            assert_eq!(
                call(&tool, json!({"action": "get", "name": name}))
                    .await
                    .unwrap(),
                "note"
            );
            call(&tool, json!({"action": "delete", "name": name}))
                .await
                .unwrap();
        }
        call(
            &tool,
            json!({"action": "put", "name": "keep", "content": "untouched"}),
        )
        .await
        .unwrap();
        call(&tool, json!({"action": "delete", "name": "other"}))
            .await
            .unwrap();
        assert_eq!(
            db::get(&conn, "reminders".into()).await.unwrap().as_deref(),
            Some("keep")
        );
        assert_eq!(
            call(&tool, json!({"action": "get", "name": "keep"}))
                .await
                .unwrap(),
            "untouched"
        );
    }

    #[tokio::test]
    async fn database_errors_are_reported_for_all_actions() {
        let (_, tool) = setup().await;
        tool.0
            .call(|db| db.execute_batch("DROP TABLE kv_store").map_err(Into::into))
            .await
            .unwrap();
        for args in [
            json!({"action": "get", "name": "test"}),
            json!({"action": "put", "name": "test", "content": "text"}),
            json!({"action": "delete", "name": "test"}),
        ] {
            assert!(
                call(&tool, args)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("Notes database error")
            );
        }
    }

    #[test]
    fn validates_argument_shapes() {
        for args in [
            json!({"action": "get"}),
            json!({"action": "put", "name": "test"}),
            json!({"action": "put", "name": "test", "content": 42}),
            json!({"action": "get", "name": "test", "content": "unexpected"}),
            json!({"action": "delete", "name": "test", "extra": true}),
            json!({"action": "unknown", "name": "test"}),
        ] {
            assert!(serde_json::from_value::<NotesArgs>(args).is_err());
        }
    }
}
