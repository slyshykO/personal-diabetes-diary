use anyhow::{Context, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) fn store_path() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::temp_dir().join("pdd-bot-reminders.json")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/run/pdd-bot-reminders.json")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reminder {
    pub chat_id: i64,
    pub next_at: DateTime<Utc>,
    pub next_number: u32,
    pub count: u32,
    pub interval_minutes: u64,
}

impl Reminder {
    fn interval(&self) -> anyhow::Result<Duration> {
        i64::try_from(self.interval_minutes)
            .ok()
            .and_then(Duration::try_minutes)
            .context("invalid reminder interval")
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.chat_id != 0, "invalid reminder chat ID");
        ensure!(
            self.next_number > 0 && self.next_number <= self.count,
            "invalid reminder progress"
        );
        ensure!(self.next_at.timestamp() > 0, "invalid reminder timestamp");
        self.next_at
            .checked_add_signed(self.interval()?)
            .context("reminder interval exceeds timestamp range")?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreFile {
    version: u32,
    reminders: Vec<Reminder>,
}

#[derive(Debug)]
pub(super) struct ReminderStore {
    path: PathBuf,
    // Generations are process-local: an old task must never update a replacement reminder.
    reminders: BTreeMap<i64, (u64, Reminder)>,
    generation: u64,
}

impl ReminderStore {
    pub fn open(
        path: &Path,
        allowed_chat_ids: &HashSet<i64>,
        enabled: bool,
    ) -> anyhow::Result<Self> {
        let file = match fs_err::read(path) {
            Ok(bytes) => serde_json::from_slice::<StoreFile>(&bytes)
                .with_context(|| format!("invalid reminder store {}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoreFile {
                version: 1,
                reminders: Vec::new(),
            },
            Err(error) => return Err(error.into()),
        };
        ensure!(
            file.version == 1,
            "unsupported reminder store version {}",
            file.version
        );
        let mut reminders = BTreeMap::new();
        for reminder in file.reminders {
            reminder.validate()?;
            ensure!(
                reminders.insert(reminder.chat_id, (0, reminder)).is_none(),
                "duplicate chat in reminder store"
            );
        }
        reminders.retain(|chat_id, _| enabled && allowed_chat_ids.contains(chat_id));
        let store = Self {
            path: path.to_owned(),
            reminders,
            generation: 0,
        };
        // Also verify that the store is writable before accepting new messages.
        store.save(&store.reminders)?;
        Ok(store)
    }

    pub fn pending(&self) -> Vec<(u64, Reminder)> {
        self.reminders.values().cloned().collect()
    }

    pub fn get(&self, chat_id: i64, generation: u64) -> Option<Reminder> {
        self.reminders
            .get(&chat_id)
            .filter(|(current, _)| *current == generation)
            .map(|(_, reminder)| reminder.clone())
    }

    pub fn schedule(&mut self, reminder: Reminder) -> anyhow::Result<u64> {
        reminder.validate()?;
        let generation = self
            .generation
            .checked_add(1)
            .context("reminder generation overflow")?;
        let mut updated = self.reminders.clone();
        updated.insert(reminder.chat_id, (generation, reminder));
        self.save(&updated)?;
        self.reminders = updated;
        self.generation = generation;
        Ok(generation)
    }

    pub fn cancel(&mut self, chat_id: i64) -> anyhow::Result<()> {
        if !self.reminders.contains_key(&chat_id) {
            return Ok(());
        }
        let mut updated = self.reminders.clone();
        updated.remove(&chat_id);
        self.save(&updated)?;
        self.reminders = updated;
        Ok(())
    }

    pub fn mark_sent(
        &mut self,
        chat_id: i64,
        generation: u64,
        sent_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let Some(mut reminder) = self.get(chat_id, generation) else {
            return Ok(());
        };
        let mut updated = self.reminders.clone();
        if reminder.next_number == reminder.count {
            updated.remove(&chat_id);
        } else {
            reminder.next_number += 1;
            // Preserve spacing when a reminder was overdue at startup.
            reminder.next_at = sent_at
                .checked_add_signed(reminder.interval()?)
                .context("next reminder exceeds timestamp range")?;
            reminder.validate()?;
            updated.insert(chat_id, (generation, reminder));
        }
        self.save(&updated)?;
        self.reminders = updated;
        Ok(())
    }

    fn save(&self, reminders: &BTreeMap<i64, (u64, Reminder)>) -> anyhow::Result<()> {
        let file = StoreFile {
            version: 1,
            reminders: reminders
                .values()
                .map(|(_, reminder)| reminder.clone())
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&file)?;
        let tmp = self
            .path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        let result = (|| -> anyhow::Result<()> {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options.open(&tmp)?;
            output.write_all(&bytes)?;
            output.sync_all()?;
            fs_err::rename(&tmp, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs_err::remove_file(&tmp);
        }
        result.with_context(|| format!("failed to save reminder store {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "pdd-reminders-test-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> PathBuf {
            self.0.join("reminders.json")
        }

        fn open(&self) -> ReminderStore {
            ReminderStore::open(&self.path(), &HashSet::from([123, 456]), true).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn reminder() -> Reminder {
        Reminder {
            chat_id: 123,
            next_at: Utc.with_ymd_and_hms(2026, 11, 23, 12, 30, 0).unwrap(),
            next_number: 1,
            count: 3,
            interval_minutes: 15,
        }
    }

    #[test]
    fn restores_schedule_and_progress_after_restarts() {
        let fixture = Fixture::new();
        let expected = reminder();
        fixture.open().schedule(expected.clone()).unwrap();

        let mut restored = fixture.open();
        let (generation, pending) = restored.pending().pop().unwrap();
        assert_eq!(pending, expected);
        let sent_at = expected.next_at + Duration::hours(1);
        restored.mark_sent(123, generation, sent_at).unwrap();

        let mut restored = fixture.open();
        let (generation, pending) = restored.pending().pop().unwrap();
        assert_eq!(pending.next_number, 2);
        assert_eq!(pending.next_at, sent_at + Duration::minutes(15));
        restored
            .mark_sent(123, generation, pending.next_at)
            .unwrap();

        let mut restored = fixture.open();
        let (generation, pending) = restored.pending().pop().unwrap();
        assert_eq!(pending.next_number, 3);
        restored
            .mark_sent(123, generation, pending.next_at)
            .unwrap();
        assert!(fixture.open().pending().is_empty());
    }

    #[test]
    fn cancellation_is_persisted_without_removing_other_chats() {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        store.schedule(reminder()).unwrap();
        let other = Reminder {
            chat_id: 456,
            ..reminder()
        };
        store.schedule(other.clone()).unwrap();
        store.cancel(123).unwrap();
        let restored = fixture.open().pending();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].1, other);
    }

    #[test]
    fn obsolete_task_cannot_advance_or_remove_replacement() {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        let old_generation = store.schedule(reminder()).unwrap();
        store.cancel(123).unwrap();
        let new_generation = store.schedule(reminder()).unwrap();
        store.mark_sent(123, old_generation, Utc::now()).unwrap();
        assert!(store.get(123, old_generation).is_none());
        assert_eq!(store.get(123, new_generation), Some(reminder()));
        assert_eq!(fixture.open().pending()[0].1, reminder());
    }

    #[test]
    fn invalid_store_is_rejected_without_overwriting_it() {
        let fixture = Fixture::new();
        let valid = serde_json::to_value(StoreFile {
            version: 1,
            reminders: vec![reminder()],
        })
        .unwrap();
        let mut invalid_files = vec![String::from("{broken"), String::from("{}")];
        for (field, value) in [
            ("chat_id", serde_json::json!(0)),
            ("next_at", serde_json::json!("not a timestamp")),
            ("next_at", serde_json::json!("1960-01-01T00:00:00Z")),
            ("count", serde_json::json!(0)),
            ("next_number", serde_json::json!(0)),
            ("next_number", serde_json::json!(4)),
            ("interval_minutes", serde_json::json!(u64::MAX)),
            ("unexpected", serde_json::json!(true)),
        ] {
            let mut invalid = valid.clone();
            invalid["reminders"][0][field] = value;
            invalid_files.push(invalid.to_string());
        }
        let mut unsupported = valid.clone();
        unsupported["version"] = serde_json::json!(2);
        invalid_files.push(unsupported.to_string());
        let mut duplicate = valid.clone();
        duplicate["reminders"]
            .as_array_mut()
            .unwrap()
            .push(valid["reminders"][0].clone());
        invalid_files.push(duplicate.to_string());

        for invalid in invalid_files {
            std::fs::write(fixture.path(), &invalid).unwrap();
            assert!(ReminderStore::open(&fixture.path(), &HashSet::from([123]), true).is_err());
            assert_eq!(std::fs::read_to_string(fixture.path()).unwrap(), invalid);
        }
    }

    #[test]
    fn removes_disallowed_chats_and_disabled_reminders_on_restore() {
        let fixture = Fixture::new();
        fixture.open().schedule(reminder()).unwrap();
        let store = ReminderStore::open(&fixture.path(), &HashSet::from([456]), true).unwrap();
        assert!(store.pending().is_empty());
        assert!(fixture.open().pending().is_empty());

        fixture.open().schedule(reminder()).unwrap();
        let store = ReminderStore::open(&fixture.path(), &HashSet::from([123]), false).unwrap();
        assert!(store.pending().is_empty());
        assert!(fixture.open().pending().is_empty());
    }

    #[test]
    fn failed_write_keeps_previous_in_memory_and_saved_state() {
        let fixture = Fixture::new();
        let mut store = fixture.open();
        let generation = store.schedule(reminder()).unwrap();
        store.path = fixture.0.join("directory");
        std::fs::create_dir(&store.path).unwrap();
        assert!(store.cancel(123).is_err());
        assert_eq!(store.get(123, generation), Some(reminder()));
        assert_eq!(fixture.open().pending()[0].1, reminder());
    }

    #[cfg(unix)]
    #[test]
    fn store_is_private_to_service_user() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new();
        fixture.open();
        assert_eq!(
            std::fs::metadata(fixture.path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[tokio::test]
    async fn restored_overdue_reminder_is_sent_with_saved_number_and_removed() {
        use crate::tgbot::{TgBotState, run_after_meal_reminders};
        use axum::{Json, Router, routing::post};
        use std::sync::Arc;
        use teloxide::prelude::*;
        use tokio::sync::{Mutex, watch};

        let fixture = Fixture::new();
        fixture
            .open()
            .schedule(Reminder {
                next_at: Utc::now() - Duration::minutes(1),
                next_number: 3,
                ..reminder()
            })
            .unwrap();
        let restored = fixture.open();
        let (generation, _) = restored.pending().pop().unwrap();
        let (_shutdown, receiver) = watch::channel(());
        let state = TgBotState {
            pending_by_chat: Default::default(),
            after_meal_reminders: Arc::new(Mutex::new(restored)),
            reminder_shutdown: receiver,
            allowed_chat_ids: HashSet::from([ChatId(123)]),
            data_dir: fixture.0.clone(),
            input_tz: chrono_tz::UTC,
            glucose_after_meal_reminder_minutes: 150,
            glucose_after_meal_reminder_count: 3,
            glucose_after_meal_reminder_interval_minutes: 15,
        };
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let router = Router::new().route(
            "/{*path}",
            post(move |Json(payload): Json<serde_json::Value>| {
                let captured = Arc::clone(&captured);
                async move {
                    captured.lock().await.push(payload);
                    Json(serde_json::json!({
                        "ok": true,
                        "result": {
                            "message_id": 1,
                            "date": Utc::now().timestamp(),
                            "chat": { "id": 123, "type": "private", "first_name": "Test" },
                            "text": "reminder"
                        }
                    }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bot = Bot::new("test-token").set_api_url(
            format!("http://{}/", listener.local_addr().unwrap())
                .parse()
                .unwrap(),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            run_after_meal_reminders(&bot, &state, ChatId(123), generation),
        )
        .await;
        server.abort();
        result.unwrap().unwrap();
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0]["text"],
            "Time to measure glucose after meal. Reminder 3/3."
        );
        assert!(fixture.open().pending().is_empty());
    }
}
