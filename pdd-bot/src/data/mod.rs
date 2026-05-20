use chrono::Utc;
use std::path::{Path, PathBuf};

const MEDICATIONS_FILE: &str = "medications.txt";
const MEDICATION_LOG_FILE: &str = "medication_log.csv";
const GLUCOSE_LOG_FILE: &str = "glucose.csv";
const WEIGHT_LOG_FILE: &str = "weight.csv";

#[derive(Debug, Clone, Copy)]
pub(crate) enum GlucoseTag {
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

pub(crate) fn ensure_data_dir(data_dir: &Path) -> anyhow::Result<()> {
    fs_err::create_dir_all(data_dir)?;
    Ok(())
}

pub(crate) fn normalize_medication_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn load_medications(data_dir: &Path, chat_id: i64) -> anyhow::Result<Vec<String>> {
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

pub(crate) fn append_medication_name(
    data_dir: &Path,
    chat_id: i64,
    name: &str,
) -> anyhow::Result<()> {
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

pub(crate) fn append_medication_log_csv(
    data_dir: &Path,
    chat_id: i64,
    medication: &str,
) -> anyhow::Result<()> {
    let file = user_data_dir(data_dir, chat_id).join(MEDICATION_LOG_FILE);
    append_line_if_needed(&file, "timestamp,chat_id,medication")?;
    let ts = Utc::now().to_rfc3339();
    let escaped_medication = medication.replace('"', "\"\"");
    append_csv_line(&file, &format!("{ts},{chat_id},\"{escaped_medication}\""))
}

pub(crate) fn append_weight_csv(data_dir: &Path, chat_id: i64, value: f64) -> anyhow::Result<()> {
    let file = user_data_dir(data_dir, chat_id).join(WEIGHT_LOG_FILE);
    append_line_if_needed(&file, "timestamp,chat_id,value_kg")?;
    let ts = Utc::now().to_rfc3339();
    append_csv_line(&file, &format!("{ts},{chat_id},{value}"))
}

pub(crate) fn append_glucose_csv(
    data_dir: &Path,
    chat_id: i64,
    tag: GlucoseTag,
    value: f64,
    timestamp: Option<&str>,
    note: Option<&str>,
) -> anyhow::Result<()> {
    let file = user_data_dir(data_dir, chat_id).join(GLUCOSE_LOG_FILE);
    append_line_if_needed(&file, "timestamp,chat_id,tag,value_mmol_l,note")?;
    let ts = match timestamp {
        Some(raw) => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
            .unwrap_or_else(|_| Utc::now().to_rfc3339()),
        None => Utc::now().to_rfc3339(),
    };
    let escaped_note = csv_escape(note.unwrap_or(""));
    append_csv_line(
        &file,
        &format!(
            "{ts},{chat_id},{},{value},\"{escaped_note}\"",
            tag.as_csv_tag()
        ),
    )
}

fn user_data_dir(data_dir: &Path, chat_id: i64) -> PathBuf {
    data_dir.join(chat_id.to_string())
}

fn medications_path(data_dir: &Path, chat_id: i64) -> PathBuf {
    user_data_dir(data_dir, chat_id).join(MEDICATIONS_FILE)
}

fn csv_escape(value: &str) -> String {
    value.replace('"', "\"\"")
}

fn append_line_if_needed(path: &Path, header: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    if !path.exists() {
        fs_err::write(path, format!("{header}\n"))?;
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
