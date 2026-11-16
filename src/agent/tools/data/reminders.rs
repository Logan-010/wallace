use super::db;
use jiff::{ZonedDateTime, civil::Weekday};
use rig::tool::{Tool, ToolContext};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, error::Error, fmt, sync::Arc, time::Duration};
use tokio::sync::{Mutex, mpsc};
use tokio_rusqlite::Connection;

const STORAGE_KEY: &str = "reminders";
type Reminders = BTreeMap<String, Reminder>;

#[derive(Clone)]
pub struct RemindersTool {
    db: Connection,
    tx: mpsc::Sender<String>,
    reminders: Arc<Mutex<Reminders>>,
}

impl RemindersTool {
    pub async fn new(conn: &Connection) -> color_eyre::Result<(Self, mpsc::Receiver<String>)> {
        let (tool, rx) = Self::load(conn).await?;
        let scheduler = tool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = scheduler.tx.closed() => break,
                    _ = interval.tick() => {
                        match scheduler.due(&ZonedDateTime::now()).await {
                            Ok(names) => {
                                for name in names {
                                    if scheduler.tx.send(name).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(error) => tracing::warn!("Failed to check reminders: {error:?}"),
                        }
                    }
                }
            }
        });
        Ok((tool, rx))
    }

    async fn load(conn: &Connection) -> color_eyre::Result<(Self, mpsc::Receiver<String>)> {
        let reminders = match db::get(conn, STORAGE_KEY.to_owned()).await? {
            Some(value) => serde_json::from_str(&value)?,
            None => Reminders::new(),
        };
        let (tx, rx) = mpsc::channel(100);
        Ok((
            Self {
                db: conn.clone(),
                tx,
                reminders: Arc::new(Mutex::new(reminders)),
            },
            rx,
        ))
    }

    async fn save(&self, reminders: &Reminders) -> color_eyre::Result<()> {
        if reminders.is_empty() {
            db::remove(&self.db, STORAGE_KEY.to_owned()).await?;
        } else {
            db::put(
                &self.db,
                STORAGE_KEY.to_owned(),
                serde_json::to_string(reminders)?,
            )
            .await?;
        }
        Ok(())
    }

    async fn due(&self, now: &ZonedDateTime) -> color_eyre::Result<Vec<String>> {
        let mut reminders = self.reminders.lock().await;
        let mut updated = reminders.clone();
        let date = now.date().to_string();
        let time = format!("{:02}:{:02}", now.hour(), now.minute());
        let mut names = Vec::new();
        for (name, reminder) in &mut updated {
            if reminder.enabled
                && reminder
                    .days
                    .iter()
                    .any(|day| day.weekday() == now.weekday())
                && reminder.time == time
                && reminder.last_fired.as_ref() != Some(&date)
            {
                reminder.last_fired = Some(date.clone());
                names.push(name.clone());
            }
        }
        if !names.is_empty() {
            self.save(&updated).await?;
            *reminders = updated;
        }
        Ok(names)
    }

    async fn apply(&self, args: ReminderArgs) -> color_eyre::Result<String> {
        let name = match &args {
            ReminderArgs::Create { name, .. }
            | ReminderArgs::Enable { name }
            | ReminderArgs::Disable { name }
            | ReminderArgs::Delete { name } => name.trim().to_owned(),
        };
        color_eyre::eyre::ensure!(!name.is_empty(), "Reminder name must not be empty.");
        let mut reminders = self.reminders.lock().await;
        let mut updated = reminders.clone();
        let action = match args {
            ReminderArgs::Create { days, time, .. } => {
                color_eyre::eyre::ensure!(!days.is_empty(), "Specify at least one weekday.");
                color_eyre::eyre::ensure!(valid_time(&time), "Time must be HH:MM (00:00–23:59).");
                color_eyre::eyre::ensure!(
                    !updated.contains_key(&name),
                    "A reminder named '{name}' already exists; delete it before replacing it."
                );
                updated.insert(
                    name.clone(),
                    Reminder {
                        days,
                        time,
                        enabled: true,
                        last_fired: None,
                    },
                );
                "created"
            }
            ReminderArgs::Enable { .. } | ReminderArgs::Disable { .. } => {
                let enabled = matches!(args, ReminderArgs::Enable { .. });
                let reminder = updated
                    .get_mut(&name)
                    .ok_or_else(|| color_eyre::eyre::eyre!("No reminder named '{name}'."))?;
                reminder.enabled = enabled;
                if enabled { "enabled" } else { "disabled" }
            }
            ReminderArgs::Delete { .. } => {
                color_eyre::eyre::ensure!(
                    updated.remove(&name).is_some(),
                    "No reminder named '{name}'."
                );
                "deleted"
            }
        };
        self.save(&updated).await?;
        *reminders = updated;
        Ok(format!("Reminder '{name}' {action}."))
    }
}

fn valid_time(time: &str) -> bool {
    let bytes = time.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && [0, 1, 3, 4].iter().all(|&i| bytes[i].is_ascii_digit())
        && time[..2].parse::<u8>().is_ok_and(|hour| hour < 24)
        && time[3..].parse::<u8>().is_ok_and(|minute| minute < 60)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Reminder {
    days: Vec<Day>,
    time: String,
    enabled: bool,
    last_fired: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Day {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Day {
    fn weekday(&self) -> Weekday {
        match self {
            Self::Monday => Weekday::Monday,
            Self::Tuesday => Weekday::Tuesday,
            Self::Wednesday => Weekday::Wednesday,
            Self::Thursday => Weekday::Thursday,
            Self::Friday => Weekday::Friday,
            Self::Saturday => Weekday::Saturday,
            Self::Sunday => Weekday::Sunday,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReminderArgs {
    Create {
        name: String,
        days: Vec<Day>,
        time: String,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Delete {
        name: String,
    },
}

#[derive(Debug)]
pub struct ReminderError(String);

impl fmt::Display for ReminderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for ReminderError {}

impl Tool for RemindersTool {
    const NAME: &'static str = "reminders";
    type Args = ReminderArgs;
    type Output = String;
    type Error = ReminderError;

    fn description(&self) -> String {
        "Create, enable, disable or delete a named weekly reminder. Create requires days (lowercase weekday names) and time (24-hour HH:MM in the system's local timezone), and starts enabled. Other actions require only the name. Names must be unique. Reminders persist across restarts and announce once on each selected day at the specified minute while the application runs. Missed occurrences are not replayed; nonexistent DST times are skipped and repeated times fire once."
            .to_owned()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "enable", "disable", "delete"] },
                "name": { "type": "string", "minLength": 1 },
                "days": {
                    "type": "array", "minItems": 1,
                    "description": "Required for create: weekdays on which to repeat",
                    "items": { "type": "string", "enum": ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"] }
                },
                "time": {
                    "type": "string",
                    "description": "Required for create: local time in 24-hour HH:MM format",
                    "pattern": "^([01][0-9]|2[0-3]):[0-5][0-9]$"
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
    ) -> Result<String, Self::Error> {
        self.apply(args)
            .await
            .map_err(|error| ReminderError(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn setup() -> (Connection, RemindersTool) {
        let conn = Connection::open_in_memory().await.unwrap();
        db::migrate(&conn).await.unwrap();
        let (tool, _rx) = RemindersTool::load(&conn).await.unwrap();
        (conn, tool)
    }

    async fn call(tool: &RemindersTool, args: serde_json::Value) -> Result<String, ReminderError> {
        tool.call(
            &mut ToolContext::default(),
            serde_json::from_value(args).unwrap(),
        )
        .await
    }

    async fn create(tool: &RemindersTool) {
        call(
            tool,
            json!({
                "action": "create", "name": " medicine ",
                "days": ["monday", "wednesday"], "time": "09:30"
            }),
        )
        .await
        .unwrap();
    }

    fn at(value: &str) -> ZonedDateTime {
        value.parse().unwrap()
    }

    #[tokio::test]
    async fn creates_and_persists_using_key_value_store() {
        let (conn, tool) = setup().await;
        db::put(&conn, "unrelated".into(), "keep".into())
            .await
            .unwrap();
        create(&tool).await;
        let stored: Reminders =
            serde_json::from_str(&db::get(&conn, STORAGE_KEY.into()).await.unwrap().unwrap())
                .unwrap();
        assert!(stored["medicine"].enabled);
        assert_eq!(stored["medicine"].time, "09:30");
        assert_eq!(stored["medicine"].days.len(), 2);

        let (reloaded, _) = RemindersTool::load(&conn).await.unwrap();
        assert_eq!(
            reloaded
                .due(&at("2026-11-16T09:30:00Z[UTC]"))
                .await
                .unwrap(),
            ["medicine"]
        );
        assert_eq!(
            db::get(&conn, "unrelated".into()).await.unwrap().as_deref(),
            Some("keep")
        );
    }

    #[tokio::test]
    async fn repeats_only_on_selected_days_and_minute_once_per_date() {
        let (_, tool) = setup().await;
        create(&tool).await;
        for time in ["2026-11-16T09:29:59Z[UTC]", "2026-11-17T09:30:00Z[UTC]"] {
            assert!(tool.due(&at(time)).await.unwrap().is_empty());
        }
        assert_eq!(
            tool.due(&at("2026-11-18T09:30:00Z[UTC]")).await.unwrap(),
            ["medicine"]
        );
        assert!(
            tool.due(&at("2026-11-18T09:30:59Z[UTC]"))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            tool.due(&at("2026-11-18T09:31:00Z[UTC]"))
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            tool.due(&at("2026-11-23T09:30:00Z[UTC]")).await.unwrap(),
            ["medicine"]
        );
    }

    #[tokio::test]
    async fn enable_disable_delete_persist_and_control_firing() {
        let (conn, tool) = setup().await;
        create(&tool).await;
        call(&tool, json!({"action": "disable", "name": "medicine"}))
            .await
            .unwrap();
        let (reloaded, _) = RemindersTool::load(&conn).await.unwrap();
        let now = at("2026-11-16T09:30:00Z[UTC]");
        assert!(reloaded.due(&now).await.unwrap().is_empty());
        call(&reloaded, json!({"action": "enable", "name": "medicine"}))
            .await
            .unwrap();
        assert_eq!(reloaded.due(&now).await.unwrap(), ["medicine"]);
        call(&reloaded, json!({"action": "disable", "name": "medicine"}))
            .await
            .unwrap();
        call(&reloaded, json!({"action": "enable", "name": "medicine"}))
            .await
            .unwrap();
        assert!(reloaded.due(&now).await.unwrap().is_empty());
        call(&reloaded, json!({"action": "delete", "name": "medicine"}))
            .await
            .unwrap();
        assert!(
            reloaded
                .due(&at("2026-11-18T09:30:00Z[UTC]"))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(db::get(&conn, STORAGE_KEY.into()).await.unwrap().is_none());
        assert!(
            RemindersTool::load(&conn)
                .await
                .unwrap()
                .0
                .reminders
                .lock()
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn restart_does_not_repeat_an_already_fired_occurrence() {
        let (conn, tool) = setup().await;
        create(&tool).await;
        let now = at("2026-11-16T09:30:00Z[UTC]");
        assert_eq!(tool.due(&now).await.unwrap(), ["medicine"]);
        let (reloaded, _) = RemindersTool::load(&conn).await.unwrap();
        assert!(reloaded.due(&now).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn repeated_dst_hour_fires_once_in_local_timezone() {
        let (_, tool) = setup().await;
        call(
            &tool,
            json!({
                "action": "create", "name": "clock", "days": ["sunday"], "time": "01:30"
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            tool.due(&at("2026-11-01T01:30:00-04:00[America/New_York]"))
                .await
                .unwrap(),
            ["clock"]
        );
        assert!(
            tool.due(&at("2026-11-01T01:30:00-05:00[America/New_York]"))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn rejects_invalid_operations_without_changing_storage() {
        let (conn, tool) = setup().await;
        create(&tool).await;
        let original = db::get(&conn, STORAGE_KEY.into()).await.unwrap();
        for args in [
            json!({"action": "create", "name": "medicine", "days": ["monday"], "time": "12:00"}),
            json!({"action": "create", "name": " ", "days": ["monday"], "time": "12:00"}),
            json!({"action": "create", "name": "new", "days": [], "time": "12:00"}),
            json!({"action": "create", "name": "new", "days": ["monday"], "time": "24:00"}),
            json!({"action": "enable", "name": "missing"}),
            json!({"action": "disable", "name": "missing"}),
            json!({"action": "delete", "name": "missing"}),
        ] {
            assert!(call(&tool, args).await.is_err());
            assert_eq!(db::get(&conn, STORAGE_KEY.into()).await.unwrap(), original);
        }
    }

    #[tokio::test]
    async fn deleting_one_reminder_preserves_others() {
        let (conn, tool) = setup().await;
        create(&tool).await;
        call(
            &tool,
            json!({
                "action": "create", "name": "other", "days": ["monday"], "time": "09:30"
            }),
        )
        .await
        .unwrap();
        call(&tool, json!({"action": "delete", "name": "medicine"}))
            .await
            .unwrap();
        let (reloaded, _) = RemindersTool::load(&conn).await.unwrap();
        assert_eq!(
            reloaded
                .due(&at("2026-11-16T09:30:00Z[UTC]"))
                .await
                .unwrap(),
            ["other"]
        );
    }

    #[tokio::test]
    async fn failed_database_write_does_not_change_live_schedule() {
        let (_, tool) = setup().await;
        create(&tool).await;
        tool.db
            .call(|db| db.execute_batch("DROP TABLE kv_store").map_err(Into::into))
            .await
            .unwrap();
        assert!(
            call(&tool, json!({"action": "disable", "name": "medicine"}))
                .await
                .is_err()
        );
        assert!(tool.reminders.lock().await["medicine"].enabled);
        assert!(tool.due(&at("2026-11-16T09:30:00Z[UTC]")).await.is_err());
        assert!(tool.reminders.lock().await["medicine"].last_fired.is_none());
    }

    #[tokio::test]
    async fn scheduler_sends_name_on_notification_channel() {
        let conn = Connection::open_in_memory().await.unwrap();
        db::migrate(&conn).await.unwrap();
        let now = ZonedDateTime::now();
        let reminders = BTreeMap::from([(
            "scheduled".to_owned(),
            Reminder {
                days: vec![
                    Day::Monday,
                    Day::Tuesday,
                    Day::Wednesday,
                    Day::Thursday,
                    Day::Friday,
                    Day::Saturday,
                    Day::Sunday,
                ],
                time: format!("{:02}:{:02}", now.hour(), now.minute()),
                enabled: true,
                last_fired: None,
            },
        )]);
        db::put(
            &conn,
            STORAGE_KEY.into(),
            serde_json::to_string(&reminders).unwrap(),
        )
        .await
        .unwrap();
        let (_tool, mut rx) = RemindersTool::new(&conn).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .as_deref(),
            Some("scheduled")
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn validates_time_format_and_argument_shapes() {
        for time in ["00:00", "09:30", "23:59"] {
            assert!(valid_time(time));
        }
        for time in [
            "", "9:30", "24:00", "12:60", "12:30:00", "ab:cd", "０:00", "09:30 ",
        ] {
            assert!(!valid_time(time));
        }
        for args in [
            json!({"action": "create", "name": "test", "time": "09:00"}),
            json!({"action": "create", "name": "test", "days": ["funday"], "time": "09:00"}),
            json!({"action": "enable", "name": "test", "time": "09:00"}),
            json!({"action": "unknown", "name": "test"}),
            json!({"action": "delete"}),
        ] {
            assert!(serde_json::from_value::<ReminderArgs>(args).is_err());
        }
    }
}
