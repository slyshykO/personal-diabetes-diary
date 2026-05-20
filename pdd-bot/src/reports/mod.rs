use anyhow::Context;
use chrono::{DateTime, Local, Timelike};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};

const GLUCOSE_LOG_FILE: &str = "glucose.csv";

const BREAKFAST_START: u32 = minutes(5, 0);
const BREAKFAST_END: u32 = minutes(11, 0);
const DINNER_START: u32 = minutes(11, 0);
const DINNER_END: u32 = minutes(17, 0);
const SUPPER_START: u32 = minutes(17, 0);
const SUPPER_END: u32 = minutes(23, 0);

// If ranges overlap, the first matching meal in this list wins.
const MEAL_WINDOWS: &[MealWindow] = &[
    MealWindow {
        meal: Meal::Breakfast,
        start: BREAKFAST_START,
        end: BREAKFAST_END,
    },
    MealWindow {
        meal: Meal::Diner,
        start: DINNER_START,
        end: DINNER_END,
    },
    MealWindow {
        meal: Meal::Supper,
        start: SUPPER_START,
        end: SUPPER_END,
    },
];

pub fn glucose_report(data_dir: &Path, chat_id: i64) -> anyhow::Result<PathBuf> {
    let user_dir = data_dir.join(chat_id.to_string());
    fs_err::create_dir_all(&user_dir)?;

    let glucose_file = user_dir.join(GLUCOSE_LOG_FILE);
    let entries = read_glucose_entries(&glucose_file)
        .with_context(|| format!("failed to read {}", glucose_file.display()))?;
    let report = build_report(&entries);
    write_unique_report(&user_dir, &report)
}

#[derive(Debug, Clone, Copy)]
struct MealWindow {
    meal: Meal,
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, Copy)]
enum Meal {
    Breakfast,
    Diner,
    Supper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlucoseTag {
    BeforeMeal,
    AfterMeal,
}

#[derive(Debug)]
struct GlucoseEntry {
    timestamp: DateTime<Local>,
    tag: GlucoseTag,
    value: String,
}

#[derive(Debug, Default)]
struct ReportDay {
    breakfast_before: Vec<String>,
    breakfast_after: Vec<String>,
    dinner_before: Vec<String>,
    dinner_after: Vec<String>,
    supper_before: Vec<String>,
    supper_after: Vec<String>,
    other: Vec<String>,
}

const fn minutes(hour: u32, minute: u32) -> u32 {
    hour * 60 + minute
}

fn read_glucose_entries(path: &Path) -> anyhow::Result<Vec<GlucoseEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs_err::read_to_string(path)?;
    let mut entries = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        if line_index == 0 {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }

        let fields = parse_csv_line(line)
            .with_context(|| format!("invalid CSV on line {}", line_index + 1))?;
        if fields.len() < 5 {
            continue;
        }
        if fields[1].parse::<i64>().is_err() {
            continue;
        }

        let timestamp = DateTime::parse_from_rfc3339(&fields[0])
            .with_context(|| format!("invalid timestamp on line {}", line_index + 1))?
            .with_timezone(&Local);
        let tag = match fields[2].as_str() {
            "before_meal" => GlucoseTag::BeforeMeal,
            "after_meal" => GlucoseTag::AfterMeal,
            _ => continue,
        };
        let value = fields[3].trim().to_string();
        if value.is_empty() {
            continue;
        }

        entries.push(GlucoseEntry {
            timestamp,
            tag,
            value,
        });
    }
    Ok(entries)
}

fn build_report(entries: &[GlucoseEntry]) -> String {
    let mut days = BTreeMap::<String, ReportDay>::new();

    for entry in entries {
        let date = entry.timestamp.format("%Y.%m.%d").to_string();
        let day = days.entry(date).or_default();
        let value = entry.value.clone();

        match meal_for_time(entry.timestamp.time(), entry.tag) {
            Some((Meal::Breakfast, GlucoseTag::BeforeMeal)) => day.breakfast_before.push(value),
            Some((Meal::Breakfast, GlucoseTag::AfterMeal)) => day.breakfast_after.push(value),
            Some((Meal::Diner, GlucoseTag::BeforeMeal)) => day.dinner_before.push(value),
            Some((Meal::Diner, GlucoseTag::AfterMeal)) => day.dinner_after.push(value),
            Some((Meal::Supper, GlucoseTag::BeforeMeal)) => day.supper_before.push(value),
            Some((Meal::Supper, GlucoseTag::AfterMeal)) => day.supper_after.push(value),
            None => {
                day.other.push(format!(
                    "{} ({})",
                    entry.value,
                    entry.timestamp.format("%H:%M")
                ));
            }
        }
    }

    let mut report = String::new();
    report.push_str(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Glucose report</title>
<style>
body {
    font-family: Arial, sans-serif;
    font-size: 12px;
    color: #111;
}
table {
    border-collapse: collapse;
    table-layout: fixed;
    width: auto;
    max-width: 100%;
}
th,
td {
    border: 1px solid #111;
    padding: 2px 4px;
    text-align: left;
    vertical-align: top;
    overflow-wrap: anywhere;
}
th {
    font-weight: 700;
    white-space: normal;
    text-align: center;
}
.date {
    width: 72px;
}
.value {
    width: 68px;
    text-align: center;
}
th.value {
    overflow-wrap: normal;
    white-space: nowrap;
}
.other {
    width: 120px;
}
</style>
</head>
<body>
<table>
<thead>
<tr><th class="date">Date</th><th class="value">Breakfast<br>before</th><th class="value">Breakfast<br>after</th><th class="value">Diner<br>before</th><th class="value">Diner<br>after</th><th class="value">Supper<br>before</th><th class="value">Supper<br>after</th><th class="other">Other</th></tr>
</thead>
<tbody>
"#,
    );

    for (date, day) in days {
        let _ = writeln!(
            report,
            "<tr><td class=\"date\">{}</td><td class=\"value\">{}</td><td class=\"value\">{}</td><td class=\"value\">{}</td><td class=\"value\">{}</td><td class=\"value\">{}</td><td class=\"value\">{}</td><td class=\"other\">{}</td></tr>",
            escape_html(&date),
            escape_html(&format_values(&day.breakfast_before)),
            escape_html(&format_values(&day.breakfast_after)),
            escape_html(&format_values(&day.dinner_before)),
            escape_html(&format_values(&day.dinner_after)),
            escape_html(&format_values(&day.supper_before)),
            escape_html(&format_values(&day.supper_after)),
            escape_html(&format_values(&day.other)),
        );
    }

    report.push_str("</tbody>\n</table>\n</body>\n</html>\n");
    report
}

fn meal_for_time(time: chrono::NaiveTime, tag: GlucoseTag) -> Option<(Meal, GlucoseTag)> {
    let minute = minutes(time.hour(), time.minute());
    MEAL_WINDOWS
        .iter()
        .find(|window| window.contains(minute))
        .map(|window| (window.meal, tag))
}

impl MealWindow {
    fn contains(self, minute: u32) -> bool {
        if self.start <= self.end {
            self.start <= minute && minute < self.end
        } else {
            self.start <= minute || minute < self.end
        }
    }
}

fn format_values(values: &[String]) -> String {
    values.join(", ")
}

fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn write_unique_report(user_dir: &Path, report: &str) -> anyhow::Result<PathBuf> {
    for attempt in 0..1000 {
        let now = chrono::Utc::now();
        let filename = match attempt {
            0 => format!("glucose_report_{}.html", now.format("%Y%m%d_%H%M%S%.3f")),
            _ => format!(
                "glucose_report_{}_{}.html",
                now.format("%Y%m%d_%H%M%S%.3f"),
                attempt
            ),
        };
        let path = user_dir.join(filename);
        let file = fs_err::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path);
        let mut file = match file {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        };
        file.write_all(report.as_bytes())?;
        return Ok(path);
    }

    Err(anyhow::anyhow!(
        "failed to create unique glucose report name"
    ))
}

fn parse_csv_line(line: &str) -> anyhow::Result<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(std::mem::take(&mut field));
            }
            _ => field.push(ch),
        }
    }

    if in_quotes {
        return Err(anyhow::anyhow!("unterminated quoted CSV field"));
    }

    fields.push(field);
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn build_report_places_measurements_by_meal_and_tag() {
        let entries = vec![
            entry(2026, 5, 15, 7, 30, GlucoseTag::BeforeMeal, "7.8"),
            entry(2026, 5, 15, 9, 30, GlucoseTag::AfterMeal, "7.9"),
            entry(2026, 5, 15, 12, 15, GlucoseTag::BeforeMeal, "6.5"),
            entry(2026, 5, 15, 18, 30, GlucoseTag::AfterMeal, "7.8"),
            entry(2026, 5, 15, 23, 30, GlucoseTag::BeforeMeal, "5.5"),
        ];

        let report = build_report(&entries);

        assert!(report.contains(
            "<tr><td class=\"date\">2026.05.15</td><td class=\"value\">7.8</td><td class=\"value\">7.9</td><td class=\"value\">6.5</td><td class=\"value\"></td><td class=\"value\"></td><td class=\"value\">7.8</td><td class=\"other\">5.5 (23:30)</td></tr>"
        ));
    }

    #[test]
    fn parse_csv_line_handles_quoted_fields() {
        let fields =
            parse_csv_line(r#"2026-05-15T06:30:00Z,1,before_meal,7.8,"before, ""food""""#).unwrap();

        assert_eq!(
            fields,
            vec![
                "2026-05-15T06:30:00Z",
                "1",
                "before_meal",
                "7.8",
                r#"before, "food""#,
            ]
        );
    }

    fn entry(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        tag: GlucoseTag,
        value: &str,
    ) -> GlucoseEntry {
        let timestamp = Local
            .with_ymd_and_hms(year, month, day, hour, minute, 0)
            .single()
            .unwrap();

        GlucoseEntry {
            timestamp,
            tag,
            value: value.to_string(),
        }
    }
}
