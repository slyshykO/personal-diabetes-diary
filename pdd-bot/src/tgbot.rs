use crate::data::GlucoseTag;
use crate::{args, data, reports};
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, LocalResult, NaiveDate, NaiveTime, TimeZone,
    Utc,
};
use chrono_tz::Tz;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;
use teloxide::types::{InputFile, KeyboardButton, KeyboardMarkup};
use tokio::sync::{Mutex, watch};

const DEFAULT_AFTER_MEAL_REMINDER_MINUTES: u64 = 150;
const DEFAULT_AFTER_MEAL_REMINDER_COUNT: u32 = 3;
const DEFAULT_AFTER_MEAL_REMINDER_INTERVAL_MINUTES: u64 = 15;
const BTN_GLUCOSE_BEFORE_MEAL: &str = "🩸 Glucose: Before meal";
const BTN_GLUCOSE_AFTER_MEAL: &str = "🩸 Glucose: After meal";
const BTN_WEIGHT: &str = "⚖️ Weight";
const BTN_SHOW_MENU: &str = "📋 Show menu";
const BTN_GLUCOSE_REPORT: &str = "📄 Glucose report";
const MED_BUTTON_PREFIX: &str = "💊 ";

#[derive(Debug, Clone, Copy)]
enum PendingEntry {
    GlucoseBeforeMeal,
    GlucoseAfterMeal,
    Weight,
}

#[derive(Debug, Clone)]
struct TgBotState {
    pending_by_chat: Arc<Mutex<HashMap<ChatId, PendingEntry>>>,
    after_meal_reminder_generations: Arc<Mutex<HashMap<ChatId, u64>>>,
    allowed_chat_ids: HashSet<ChatId>,
    data_dir: PathBuf,
    input_tz: Tz,
    glucose_after_meal_reminder_minutes: u64,
    glucose_after_meal_reminder_count: u32,
    glucose_after_meal_reminder_interval_minutes: u64,
}

pub(crate) async fn run(config: args::TgConfig) -> anyhow::Result<Option<watch::Sender<()>>> {
    let token_missing = config
        .tg_bot_token
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty();
    let chat_ids_missing = config.tg_chat_id.as_ref().is_none_or(Vec::is_empty);

    if token_missing || chat_ids_missing {
        let reason = match (token_missing, chat_ids_missing) {
            (true, true) => "tg_bot_token and tg_chat_id are not configured",
            (true, false) => "tg_bot_token is not configured",
            (false, true) => "tg_chat_id is not configured",
            (false, false) => unreachable!(),
        };
        tracing::info!("telegram bot disabled: {reason}");
        return Ok(None);
    }

    let (tx, rx) = watch::channel(());
    match run_inner(config, rx).await {
        Ok(()) => Ok(Some(tx)),
        Err(e) => {
            tracing::error!("bot error: {e}");
            Ok(None)
        }
    }
}

pub(crate) async fn run_inner(
    config: args::TgConfig,
    mut shutdown: watch::Receiver<()>,
) -> anyhow::Result<()> {
    let tg_bot_token = config
        .tg_bot_token
        .ok_or_else(|| anyhow::anyhow!("tg_bot_token is required in config"))?;
    let tg_chat_id = config
        .tg_chat_id
        .ok_or_else(|| anyhow::anyhow!("tg_chat_id is required in config"))?;
    let allowed_chat_ids = tg_chat_id
        .iter()
        .map(|id| {
            id.parse::<i64>()
                .map(ChatId)
                .map_err(|e| anyhow::anyhow!("invalid tg_chat_id '{id}': {e}"))
        })
        .collect::<anyhow::Result<HashSet<_>>>()?;
    let data_dir = config
        .data_dir
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));
    let input_tz_name = config
        .input_timezone
        .clone()
        .unwrap_or_else(|| "UTC".to_string());
    let input_tz = input_tz_name.parse::<Tz>().map_err(|_| {
        anyhow::anyhow!(
            "invalid input_timezone '{input_tz_name}'. Use IANA timezone, e.g. Europe/Kyiv or UTC"
        )
    })?;
    let glucose_after_meal_reminder_minutes = config
        .glucose_after_meal_reminder_minutes
        .unwrap_or(DEFAULT_AFTER_MEAL_REMINDER_MINUTES);
    let glucose_after_meal_reminder_count = config
        .glucose_after_meal_reminder_count
        .unwrap_or(DEFAULT_AFTER_MEAL_REMINDER_COUNT);
    let glucose_after_meal_reminder_interval_minutes = config
        .glucose_after_meal_reminder_interval_minutes
        .unwrap_or(DEFAULT_AFTER_MEAL_REMINDER_INTERVAL_MINUTES);
    data::ensure_data_dir(&data_dir)?;

    let state = TgBotState {
        pending_by_chat: Arc::new(Mutex::new(HashMap::new())),
        after_meal_reminder_generations: Arc::new(Mutex::new(HashMap::new())),
        allowed_chat_ids,
        data_dir,
        input_tz,
        glucose_after_meal_reminder_minutes,
        glucose_after_meal_reminder_count,
        glucose_after_meal_reminder_interval_minutes,
    };

    let bot = Bot::new(tg_bot_token);

    let shared_state = Arc::new(state);

    tokio::spawn(async move {
        tokio::select! {
            _ = shutdown.changed() => {
                tracing::info!("shutdown signal received, stopping bot");
            }
            _ = teloxide::repl(bot, move |bot: Bot, message: Message| {
                    let state = Arc::clone(&shared_state);
                    async move {
                        if let Err(err) = handle_message(bot, message, state).await {
                            tracing::error!("handler error: {err}");
                        }
                        respond(())
                    }
                }) => {
                    tracing::info!("teloxide loop exited");
                }
        } //select!
    });

    Ok(())
}

fn build_menu_keyboard(medications: &[String]) -> KeyboardMarkup {
    let mut rows = vec![
        vec![
            KeyboardButton::new(BTN_GLUCOSE_BEFORE_MEAL),
            KeyboardButton::new(BTN_GLUCOSE_AFTER_MEAL),
        ],
        vec![
            KeyboardButton::new(BTN_WEIGHT),
            KeyboardButton::new(BTN_GLUCOSE_REPORT),
        ],
        vec![KeyboardButton::new(BTN_SHOW_MENU)],
    ];

    for meds_chunk in medications.chunks(2) {
        let mut row = Vec::with_capacity(2);
        for med in meds_chunk {
            row.push(KeyboardButton::new(format!("{MED_BUTTON_PREFIX}{med}")));
        }
        rows.push(row);
    }

    KeyboardMarkup::new(rows).resize_keyboard()
}

async fn menu_keyboard(state: &TgBotState, chat_id: ChatId) -> KeyboardMarkup {
    let medications = data::load_medications(&state.data_dir, chat_id.0).unwrap_or_default();
    build_menu_keyboard(&medications)
}

async fn handle_message(bot: Bot, message: Message, state: Arc<TgBotState>) -> anyhow::Result<()> {
    let chat_id = message.chat.id;
    if !state.allowed_chat_ids.contains(&chat_id) {
        return Ok(());
    }

    let text = match message.text() {
        Some(text) => text.trim(),
        None => return Ok(()),
    };

    if text == "/help" {
        bot.send_message(chat_id, help_text())
            .reply_markup(menu_keyboard(&state, chat_id).await)
            .await?;
        return Ok(());
    }

    if is_glucose_report_command(text) {
        send_glucose_report(&bot, chat_id, &state).await?;
        return Ok(());
    }

    if let Some((tag, payload)) = parse_glucose_add_command(text) {
        let payload = payload.trim();
        if payload.is_empty() {
            bot.send_message(
                chat_id,
                "Usage:\n/addgb <value> [MM/DD hh:mm] [@note]\n/addga <value> [MM/DD hh:mm] [@note]",
            )
            .reply_markup(menu_keyboard(&state, chat_id).await)
            .await?;
            return Ok(());
        }

        let (value, timestamp, note) = match parse_glucose_payload(payload, state.input_tz) {
            Ok(ok) => ok,
            Err(msg) => {
                bot.send_message(chat_id, msg.to_string())
                    .reply_markup(menu_keyboard(&state, chat_id).await)
                    .await?;
                return Ok(());
            }
        };

        data::append_glucose_csv(
            &state.data_dir,
            chat_id.0,
            tag,
            value,
            timestamp.as_deref(),
            note.as_deref(),
        )?;
        update_after_meal_reminders(&bot, &state, chat_id, tag).await;
        bot.send_message(chat_id, "Glucose entry saved ✅")
            .reply_markup(menu_keyboard(&state, chat_id).await)
            .await?;
        return Ok(());
    }

    if let Some(name) = parse_addmed_command(text) {
        if name.is_empty() {
            bot.send_message(chat_id, "Usage: /addmed <medication name>")
                .reply_markup(menu_keyboard(&state, chat_id).await)
                .await?;
            return Ok(());
        }

        if add_medication(&state, chat_id, name).await? {
            bot.send_message(chat_id, format!("Medication added: {name}"))
                .reply_markup(menu_keyboard(&state, chat_id).await)
                .await?;
        } else {
            bot.send_message(chat_id, format!("Medication already exists: {name}"))
                .reply_markup(menu_keyboard(&state, chat_id).await)
                .await?;
        }
        return Ok(());
    }

    match text {
        "/start" | "/menu" | BTN_SHOW_MENU => {
            send_menu(&bot, chat_id, &state).await?;
            return Ok(());
        }
        BTN_GLUCOSE_REPORT => {
            send_glucose_report(&bot, chat_id, &state).await?;
            return Ok(());
        }
        BTN_GLUCOSE_BEFORE_MEAL => {
            set_pending(&state, chat_id, PendingEntry::GlucoseBeforeMeal).await;
            bot.send_message(
                chat_id,
                "Enter glucose: <value> [date time] [@note], e.g. 5.8 2/1 9:05 @before breakfast",
            )
            .reply_markup(menu_keyboard(&state, chat_id).await)
            .await?;
            return Ok(());
        }
        BTN_GLUCOSE_AFTER_MEAL => {
            set_pending(&state, chat_id, PendingEntry::GlucoseAfterMeal).await;
            bot.send_message(
                chat_id,
                "Enter glucose: <value> [date time] [@note], e.g. 7.2 2/1 11:00 @after lunch",
            )
            .reply_markup(menu_keyboard(&state, chat_id).await)
            .await?;
            return Ok(());
        }
        BTN_WEIGHT => {
            set_pending(&state, chat_id, PendingEntry::Weight).await;
            bot.send_message(chat_id, "Enter weight value (kg), for example: 78.4")
                .reply_markup(menu_keyboard(&state, chat_id).await)
                .await?;
            return Ok(());
        }
        _ => {}
    }

    if let Some(medication_name) = parse_medication_button(text) {
        if medication_exists(&state, chat_id, medication_name).await {
            data::append_medication_log_csv(&state.data_dir, chat_id.0, medication_name)?;
            bot.send_message(
                chat_id,
                format!("Medication usage saved ✅ ({medication_name})"),
            )
            .reply_markup(menu_keyboard(&state, chat_id).await)
            .await?;
        } else {
            bot.send_message(chat_id, "Unknown medication. Use /addmed <name> first.")
                .reply_markup(menu_keyboard(&state, chat_id).await)
                .await?;
        }
        return Ok(());
    }

    if let Some(pending) = get_pending(&state, chat_id).await {
        match pending {
            PendingEntry::GlucoseBeforeMeal | PendingEntry::GlucoseAfterMeal => {
                match parse_glucose_payload(text, state.input_tz) {
                    Ok((value, timestamp, note)) => {
                        let tag = match pending {
                            PendingEntry::GlucoseBeforeMeal => GlucoseTag::BeforeMeal,
                            PendingEntry::GlucoseAfterMeal => GlucoseTag::AfterMeal,
                            PendingEntry::Weight => unreachable!(),
                        };
                        data::append_glucose_csv(
                            &state.data_dir,
                            chat_id.0,
                            tag,
                            value,
                            timestamp.as_deref(),
                            note.as_deref(),
                        )?;
                        update_after_meal_reminders(&bot, &state, chat_id, tag).await;
                        clear_pending(&state, chat_id).await;
                        bot.send_message(chat_id, "Saved ✅")
                            .reply_markup(menu_keyboard(&state, chat_id).await)
                            .await?;
                    }
                    Err(msg) => {
                        bot.send_message(chat_id, msg.to_string())
                            .reply_markup(menu_keyboard(&state, chat_id).await)
                            .await?;
                    }
                }
            }
            PendingEntry::Weight => {
                if let Some(value) = parse_decimal(text) {
                    data::append_weight_csv(&state.data_dir, chat_id.0, value)?;
                    clear_pending(&state, chat_id).await;
                    bot.send_message(chat_id, "Saved ✅")
                        .reply_markup(menu_keyboard(&state, chat_id).await)
                        .await?;
                } else {
                    bot.send_message(
                        chat_id,
                        "Could not parse number. Use format like 78.4 (dot or comma).",
                    )
                    .reply_markup(menu_keyboard(&state, chat_id).await)
                    .await?;
                }
            }
        }
        return Ok(());
    }

    bot.send_message(
        chat_id,
        "Choose an action from menu. Type /menu to show buttons or /addmed <name>.",
    )
    .reply_markup(menu_keyboard(&state, chat_id).await)
    .await?;
    Ok(())
}

async fn update_after_meal_reminders(
    bot: &Bot,
    state: &Arc<TgBotState>,
    chat_id: ChatId,
    tag: GlucoseTag,
) {
    match tag {
        GlucoseTag::BeforeMeal => schedule_after_meal_reminders(bot, state, chat_id).await,
        GlucoseTag::AfterMeal => cancel_after_meal_reminders(state, chat_id).await,
    }
}

async fn schedule_after_meal_reminders(bot: &Bot, state: &Arc<TgBotState>, chat_id: ChatId) {
    let reminder_minutes = state.glucose_after_meal_reminder_minutes;
    let reminder_count = state.glucose_after_meal_reminder_count;
    if reminder_minutes == 0 || reminder_count == 0 {
        return;
    }

    let reminder_generation = next_after_meal_reminder_generation(state, chat_id).await;
    let reminder_interval_minutes = state.glucose_after_meal_reminder_interval_minutes;
    let bot = bot.clone();
    let state = Arc::clone(state);
    let now_local = Utc::now().with_timezone(&state.input_tz);
    let first_reminder_time =
        format_reminder_time(now_local, now_local + chrono_minutes(reminder_minutes));
    let reminder_message = format!(
        "Reminder set: measure glucose after meal at {} ({}).",
        first_reminder_time, state.input_tz
    );
    bot.send_message(chat_id, reminder_message)
        .reply_markup(menu_keyboard(&state, chat_id).await)
        .await
        .ok();
    tokio::spawn(async move {
        for reminder_index in 0..reminder_count {
            let delay_minutes = if reminder_index == 0 {
                reminder_minutes
            } else {
                reminder_interval_minutes
            };
            tokio::time::sleep(Duration::from_secs(delay_minutes.saturating_mul(60))).await;

            if !is_current_after_meal_reminder_generation(&state, chat_id, reminder_generation)
                .await
            {
                return;
            }

            if let Err(err) = bot
                .send_message(
                    chat_id,
                    format!(
                        "Time to measure glucose after meal. Reminder {}/{}.",
                        reminder_index + 1,
                        reminder_count
                    ),
                )
                .reply_markup(menu_keyboard(&state, chat_id).await)
                .await
            {
                tracing::error!("after meal reminder error: {err}");
            }
        }
    });
}

fn format_reminder_time(reference_time: DateTime<Tz>, reminder_time: DateTime<Tz>) -> String {
    let pattern = if reminder_time.date_naive() == reference_time.date_naive() {
        "%H:%M"
    } else {
        "%Y.%m.%d %H:%M"
    };
    reminder_time.format(pattern).to_string()
}

fn chrono_minutes(minutes: u64) -> ChronoDuration {
    ChronoDuration::minutes(minutes.min(i64::MAX as u64) as i64)
}

async fn next_after_meal_reminder_generation(state: &TgBotState, chat_id: ChatId) -> u64 {
    let mut lock = state.after_meal_reminder_generations.lock().await;
    let generation = lock.entry(chat_id).or_insert(0);
    *generation = generation.saturating_add(1);
    *generation
}

async fn cancel_after_meal_reminders(state: &TgBotState, chat_id: ChatId) {
    let mut lock = state.after_meal_reminder_generations.lock().await;
    let generation = lock.entry(chat_id).or_insert(0);
    *generation = generation.saturating_add(1);
}

async fn is_current_after_meal_reminder_generation(
    state: &TgBotState,
    chat_id: ChatId,
    reminder_generation: u64,
) -> bool {
    let lock = state.after_meal_reminder_generations.lock().await;
    lock.get(&chat_id).copied() == Some(reminder_generation)
}

async fn send_menu(bot: &Bot, chat_id: ChatId, state: &TgBotState) -> anyhow::Result<()> {
    bot.send_message(
        chat_id,
        "Diabetes diary menu:\n- Glucose before meal\n- Glucose after meal\n- Weight\n- Glucose report\n- Medications\nUse /addmed <name> to add medication button.\nUse /addgb, /addga, or /report.",
    )
    .reply_markup(menu_keyboard(state, chat_id).await)
    .await?;
    Ok(())
}

fn parse_glucose_add_command(text: &str) -> Option<(GlucoseTag, &str)> {
    let mappings = [
        ("/addgb", GlucoseTag::BeforeMeal),
        ("/add_glucose_before", GlucoseTag::BeforeMeal),
        ("/addga", GlucoseTag::AfterMeal),
        ("/add_glucose_after", GlucoseTag::AfterMeal),
    ];

    for (cmd, tag) in mappings {
        if text == cmd {
            return Some((tag, ""));
        }

        let with_space = format!("{cmd} ");
        if let Some(rest) = text.strip_prefix(&with_space) {
            return Some((tag, rest.trim()));
        }
    }

    None
}

async fn send_glucose_report(bot: &Bot, chat_id: ChatId, state: &TgBotState) -> anyhow::Result<()> {
    let path = reports::glucose_report(&state.data_dir, chat_id.0)?;
    bot.send_document(chat_id, InputFile::file(path))
        .caption("Glucose report")
        .reply_markup(menu_keyboard(state, chat_id).await)
        .await?;
    Ok(())
}

fn is_glucose_report_command(text: &str) -> bool {
    matches!(text, "/report" | "/glucose_report")
}

fn help_text() -> &'static str {
    "Commands:\n\
/menu - show menu buttons\n\
/help - show this help\n\
/report - create and send glucose report\n\
/addmed <name> - add medication button\n\
/addgb <value> [date time] [@note] - add glucose before meal\n\
/addga <value> [date time] [@note] - add glucose after meal\n\n\
Date/time examples:\n\
- 2/1 9:05\n\
- 02/01 09:05\n\
- 24/2/1 9:05\n\
- 2024/2/1 9:05\n\
If year is omitted, current year is used.\n\
Note example: @before breakfast\n\n\
Warning: data is stored as plain text CSV/TXT and is not encrypted by this bot."
}

fn parse_glucose_payload(
    payload: &str,
    input_tz: Tz,
) -> anyhow::Result<(f64, Option<String>, Option<String>)> {
    let (without_note, note) = split_note(payload);
    let mut parts = without_note.split_whitespace();
    let value_raw = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing glucose value"))?;
    let value = parse_decimal(value_raw)
        .ok_or_else(|| anyhow::anyhow!("Invalid glucose value. Example: 5.8"))?;

    let rest = parts.collect::<Vec<_>>().join(" ");
    if rest.trim().is_empty() {
        return Ok((value, None, note));
    }

    let dt = parse_flexible_datetime(&rest, input_tz).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid date/time. Examples: 2/1 9:05, 02/01 09:05, 24/2/1 9:05, 2024/2/1 9:05"
        )
    })?;
    Ok((value, Some(dt.to_rfc3339()), note))
}

fn split_note(input: &str) -> (&str, Option<String>) {
    if let Some(index) = input.find('@') {
        let before = input[..index].trim();
        let mut note = &input[index + 1..];
        if let Some(stripped) = note.strip_prefix(' ') {
            note = stripped;
        }
        if note.is_empty() {
            (before, None)
        } else {
            (before, Some(note.to_string()))
        }
    } else {
        (input.trim(), None)
    }
}

fn parse_flexible_datetime(input: &str, input_tz: Tz) -> Option<chrono::DateTime<Utc>> {
    let normalized = input.trim().replace(['-', '.'], "/");
    let mut parts = normalized.split_whitespace();
    let date_part = parts.next()?;
    let time_part = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let date_parts = date_part.split('/').collect::<Vec<_>>();
    if !(date_parts.len() == 2 || date_parts.len() == 3) {
        return None;
    }

    let time_parts = time_part.split(':').collect::<Vec<_>>();
    if time_parts.len() != 2 {
        return None;
    }

    let hour = time_parts[0].parse::<u32>().ok()?;
    let minute = time_parts[1].parse::<u32>().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }

    let (year, month, day) = if date_parts.len() == 2 {
        let month = date_parts[0].parse::<u32>().ok()?;
        let day = date_parts[1].parse::<u32>().ok()?;
        let current_year = Utc::now().with_timezone(&input_tz).year();
        (current_year, month, day)
    } else {
        let year_raw = date_parts[0].parse::<i32>().ok()?;
        let month = date_parts[1].parse::<u32>().ok()?;
        let day = date_parts[2].parse::<u32>().ok()?;

        let year = if (0..=99).contains(&year_raw) {
            2000 + year_raw
        } else {
            year_raw
        };
        (year, month, day)
    };

    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
    let naive = date.and_time(time);
    match input_tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(dt, _) => Some(dt.with_timezone(&Utc)),
        LocalResult::None => None,
    }
}

fn parse_addmed_command(text: &str) -> Option<&str> {
    for prefix in ["/addmed", "/add_medication"] {
        if text == prefix {
            return Some("");
        }
        let with_space = format!("{prefix} ");
        if let Some(rest) = text.strip_prefix(&with_space) {
            return Some(rest.trim());
        }
    }
    None
}

fn parse_medication_button(text: &str) -> Option<&str> {
    text.strip_prefix(MED_BUTTON_PREFIX).map(str::trim)
}

async fn medication_exists(state: &TgBotState, chat_id: ChatId, name: &str) -> bool {
    let normalized = data::normalize_medication_name(name);
    let medications = data::load_medications(&state.data_dir, chat_id.0).unwrap_or_default();
    medications
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
}

async fn add_medication(state: &TgBotState, chat_id: ChatId, name: &str) -> anyhow::Result<bool> {
    let normalized = data::normalize_medication_name(name);
    if normalized.is_empty() {
        return Ok(false);
    }

    let medications = data::load_medications(&state.data_dir, chat_id.0).unwrap_or_default();
    if medications
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
    {
        return Ok(false);
    }

    data::append_medication_name(&state.data_dir, chat_id.0, &normalized)?;
    Ok(true)
}

async fn set_pending(state: &TgBotState, chat_id: ChatId, pending: PendingEntry) {
    let mut lock = state.pending_by_chat.lock().await;
    lock.insert(chat_id, pending);
}

async fn get_pending(state: &TgBotState, chat_id: ChatId) -> Option<PendingEntry> {
    let lock = state.pending_by_chat.lock().await;
    lock.get(&chat_id).copied()
}

async fn clear_pending(state: &TgBotState, chat_id: ChatId) {
    let mut lock = state.pending_by_chat.lock().await;
    lock.remove(&chat_id);
}

fn parse_decimal(input: &str) -> Option<f64> {
    let normalized = input.trim().replace(',', ".");
    normalized.parse::<f64>().ok()
}
