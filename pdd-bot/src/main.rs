mod args;

use clap::Parser;
use chrono::{Datelike, Local, LocalResult, NaiveDate, NaiveTime, TimeZone};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{KeyboardButton, KeyboardMarkup};
use tokio::sync::Mutex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const BTN_GLUCOSE_BEFORE_MEAL: &str = "🩸 Glucose: Before meal";
const BTN_GLUCOSE_AFTER_MEAL: &str = "🩸 Glucose: After meal";
const BTN_WEIGHT: &str = "⚖️ Weight";
const BTN_SHOW_MENU: &str = "📋 Show menu";
const MED_BUTTON_PREFIX: &str = "💊 ";
const MEDICATIONS_FILE: &str = "medications.txt";
const MEDICATION_LOG_FILE: &str = "medication_log.csv";

#[derive(Debug, Clone, Copy)]
enum GlucoseTag {
    BeforeMeal,
    AfterMeal,
}

impl GlucoseTag {
    fn as_csv_tag(self) -> &'static str {
        match self {
            GlucoseTag::BeforeMeal => "before_meal",
            GlucoseTag::AfterMeal => "after_meal",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PendingEntry {
    GlucoseBeforeMeal,
    GlucoseAfterMeal,
    Weight,
}

#[derive(Debug, Clone)]
struct AppState {
    pending_by_chat: Arc<Mutex<HashMap<ChatId, PendingEntry>>>,
    allowed_chat_ids: HashSet<ChatId>,
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = args::Args::parse();
    match args.action {
        Some(args::Action::CheckConfig { config }) => match config_check(config).await {
            Ok(()) => {
                println!("config is ok");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("bad config: {e}");
                ExitCode::from(3)
            }
        },
        None => {
            if let Err(e) = run(args.config).await {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

async fn config_check<P: AsRef<Path> + Send>(_path: P) -> anyhow::Result<()> {
    Ok(())
}

async fn run<P: AsRef<Path> + Send>(path: P) -> anyhow::Result<()> {
    init_tracing();
    tracing::info!(
        "{}, version: {}",
        env!("CARGO_PKG_NAME"),
        args::get_version_str()
    );
    let path = path.as_ref();
    let config = args::AppConfig::from_file(path)?;
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
    fs_err::create_dir_all(&data_dir)?;

    let state = AppState {
        pending_by_chat: Arc::new(Mutex::new(HashMap::new())),
        allowed_chat_ids,
        data_dir,
    };

    let bot = Bot::new(tg_bot_token);
    tracing::info!("Running with config: {}", path.display());

    let shared_state = Arc::new(state);
    teloxide::repl(bot, move |bot: Bot, message: Message| {
        let state = Arc::clone(&shared_state);
        async move {
            if let Err(err) = handle_message(bot, message, state).await {
                tracing::error!("handler error: {err}");
            }
            respond(())
        }
    })
    .await;

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
            KeyboardButton::new(BTN_SHOW_MENU),
        ],
    ];

    for meds_chunk in medications.chunks(2) {
        let mut row = Vec::with_capacity(2);
        for med in meds_chunk {
            row.push(KeyboardButton::new(format!("{MED_BUTTON_PREFIX}{med}")));
        }
        rows.push(row);
    }

    KeyboardMarkup::new(rows)
    .resize_keyboard()
}

async fn menu_keyboard(state: &AppState, chat_id: ChatId) -> KeyboardMarkup {
    let medications = load_medications(&state.data_dir, chat_id).unwrap_or_default();
    build_menu_keyboard(&medications)
}

async fn handle_message(bot: Bot, message: Message, state: Arc<AppState>) -> anyhow::Result<()> {
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

        let (value, timestamp, note) = match parse_glucose_payload(payload) {
            Ok(ok) => ok,
            Err(msg) => {
                bot.send_message(chat_id, msg.to_string())
                    .reply_markup(menu_keyboard(&state, chat_id).await)
                    .await?;
                return Ok(());
            }
        };

        append_glucose_csv(
            &state.data_dir,
            chat_id,
            tag,
            value,
            timestamp.as_deref(),
            note.as_deref(),
        )?;
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
            append_medication_log_csv(&state.data_dir, chat_id, medication_name)?;
            bot.send_message(chat_id, format!("Medication usage saved ✅ ({medication_name})"))
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
                match parse_glucose_payload(text) {
                    Ok((value, timestamp, note)) => {
                        let tag = match pending {
                            PendingEntry::GlucoseBeforeMeal => GlucoseTag::BeforeMeal,
                            PendingEntry::GlucoseAfterMeal => GlucoseTag::AfterMeal,
                            PendingEntry::Weight => unreachable!(),
                        };
                        append_glucose_csv(
                            &state.data_dir,
                            chat_id,
                            tag,
                            value,
                            timestamp.as_deref(),
                            note.as_deref(),
                        )?;
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
                    append_measurement_csv(&state.data_dir, chat_id, pending, value)?;
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

async fn send_menu(bot: &Bot, chat_id: ChatId, state: &AppState) -> anyhow::Result<()> {
    bot.send_message(
        chat_id,
        "Diabetes diary menu:\n- Glucose before meal\n- Glucose after meal\n- Weight\n- Medications\nUse /addmed <name> to add medication button.\nUse /addgb or /addga for direct glucose entry with optional date/time.",
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

fn help_text() -> &'static str {
    "Commands:\n\
/menu - show menu buttons\n\
/help - show this help\n\
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

fn parse_glucose_payload(payload: &str) -> anyhow::Result<(f64, Option<String>, Option<String>)> {
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

    let dt = parse_flexible_datetime(&rest)
        .ok_or_else(|| {
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

fn parse_flexible_datetime(input: &str) -> Option<chrono::DateTime<Local>> {
    let normalized = input.trim().replace('-', "/").replace('.', "/");
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
        (Local::now().year(), month, day)
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
    match Local.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Some(dt),
        LocalResult::Ambiguous(dt, _) => Some(dt),
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

fn normalize_medication_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn medication_exists(state: &AppState, chat_id: ChatId, name: &str) -> bool {
    let normalized = normalize_medication_name(name);
    let medications = load_medications(&state.data_dir, chat_id).unwrap_or_default();
    medications
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
}

async fn add_medication(state: &AppState, chat_id: ChatId, name: &str) -> anyhow::Result<bool> {
    let normalized = normalize_medication_name(name);
    if normalized.is_empty() {
        return Ok(false);
    }

    let medications = load_medications(&state.data_dir, chat_id).unwrap_or_default();
    if medications
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&normalized))
    {
        return Ok(false);
    }

    append_medication_name(&state.data_dir, chat_id, &normalized)?;
    Ok(true)
}

fn load_medications(data_dir: &Path, chat_id: ChatId) -> anyhow::Result<Vec<String>> {
    let path = medications_path(data_dir, chat_id);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs_err::read_to_string(path)?;
    let mut result = Vec::new();
    for line in content.lines() {
        let name = normalize_medication_name(line);
        if name.is_empty() {
            continue;
        }
        if result
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&name))
        {
            continue;
        }
        result.push(name);
    }
    Ok(result)
}

fn user_data_dir(data_dir: &Path, chat_id: ChatId) -> PathBuf {
    data_dir.join(chat_id.0.to_string())
}

fn medications_path(data_dir: &Path, chat_id: ChatId) -> PathBuf {
    user_data_dir(data_dir, chat_id).join(MEDICATIONS_FILE)
}

fn append_medication_name(data_dir: &Path, chat_id: ChatId, name: &str) -> anyhow::Result<()> {
    let path = medications_path(data_dir, chat_id);
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    if !path.exists() {
        fs_err::write(&path, format!("{name}\n"))?;
        return Ok(());
    }
    append_csv_line(&path, name)
}

fn append_medication_log_csv(data_dir: &Path, chat_id: ChatId, medication: &str) -> anyhow::Result<()> {
    let file = user_data_dir(data_dir, chat_id).join(MEDICATION_LOG_FILE);
    append_line_if_needed(&file, "timestamp,chat_id,medication")?;
    let ts = chrono::Utc::now().to_rfc3339();
    let escaped_medication = medication.replace('"', "\"\"");
    append_csv_line(&file, &format!("{ts},{},\"{escaped_medication}\"", chat_id.0))
}

async fn set_pending(state: &AppState, chat_id: ChatId, pending: PendingEntry) {
    let mut lock = state.pending_by_chat.lock().await;
    lock.insert(chat_id, pending);
}

async fn get_pending(state: &AppState, chat_id: ChatId) -> Option<PendingEntry> {
    let lock = state.pending_by_chat.lock().await;
    lock.get(&chat_id).copied()
}

async fn clear_pending(state: &AppState, chat_id: ChatId) {
    let mut lock = state.pending_by_chat.lock().await;
    lock.remove(&chat_id);
}

fn parse_decimal(input: &str) -> Option<f64> {
    let normalized = input.trim().replace(',', ".");
    normalized.parse::<f64>().ok()
}

fn append_measurement_csv(
    data_dir: &Path,
    chat_id: ChatId,
    pending: PendingEntry,
    value: f64,
) -> anyhow::Result<()> {
    match pending {
        PendingEntry::GlucoseBeforeMeal | PendingEntry::GlucoseAfterMeal => {
            let tag = match pending {
                PendingEntry::GlucoseBeforeMeal => GlucoseTag::BeforeMeal,
                PendingEntry::GlucoseAfterMeal => GlucoseTag::AfterMeal,
                PendingEntry::Weight => unreachable!(),
            };
            append_glucose_csv(data_dir, chat_id, tag, value, None, None)?;
        }
        PendingEntry::Weight => {
            let file = user_data_dir(data_dir, chat_id).join("weight.csv");
            append_line_if_needed(&file, "timestamp,chat_id,value_kg")?;
            let ts = chrono::Utc::now().to_rfc3339();
            append_csv_line(&file, &format!("{ts},{},{}", chat_id.0, value))?;
        }
    }

    Ok(())
}

fn append_glucose_csv(
    data_dir: &Path,
    chat_id: ChatId,
    tag: GlucoseTag,
    value: f64,
    timestamp: Option<&str>,
    note: Option<&str>,
) -> anyhow::Result<()> {
    let file = user_data_dir(data_dir, chat_id).join("glucose.csv");
    append_line_if_needed(&file, "timestamp,chat_id,tag,value_mmol_l,note")?;
    let ts = timestamp
        .map(str::to_owned)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let escaped_note = csv_escape(note.unwrap_or(""));
    append_csv_line(
        &file,
        &format!("{ts},{},{},{},\"{escaped_note}\"", chat_id.0, tag.as_csv_tag(), value),
    )
}

fn csv_escape(value: &str) -> String {
    value.replace('"', "\"\"")
}

fn append_line_if_needed(path: &Path, header: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    if !path.exists() {
        let mut initial = String::from(header);
        if path.extension() == Some(OsStr::new("txt")) {
            initial.push('\n');
        } else {
            initial.push('\n');
        }
        fs_err::write(path, initial)?;
    }
    Ok(())
}

fn append_csv_line(path: &Path, line: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut file = fs_err::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "pdd_bot=debug,teloxide=debug".into()),
        ))
        .with(
            tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true),
        )
        .init();
}
