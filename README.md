# personal-diabetes-diary

Telegram bot for logging diabetes diary data into CSV files.

## Data security warning

This bot stores all diary data as plain text files (`.csv`, `.txt`) on disk.

- Data is saved **as is**.
- Data is **not encrypted** at rest by this bot.
- You should use this bot only if you are comfortable with local plain-text storage, or protect storage separately (disk encryption, access controls, backups policy).

## Quick Start

1. Create `pdd-bot/config.toml`:

```toml
tg_bot_token = "<YOUR_TELEGRAM_BOT_TOKEN>"
tg_chat_id = ["<YOUR_CHAT_ID>"]
data_dir = "data"
input_timezone = "Europe/Kyiv"
glucose_after_meal_reminder_minutes = 150
glucose_after_meal_reminder_count = 3
glucose_after_meal_reminder_interval_minutes = 15
```

2. Build and run:

```bash
cd pdd-bot
cargo run
```

3. In Telegram, open your bot and send:

- `/start` (show menu)
- `/addgb 5.8 @fasting` (example glucose before meal)

## Features

- Glucose logging with tags:
	- before meal
	- after meal
- Weight logging
- Medication list with dynamic Telegram buttons
- Medication usage logging
- Optional glucose note starting with `@`
- Optional glucose date/time in flexible formats
- Configurable after-meal glucose reminders

## Project layout

- `pdd-bot/` — Rust Telegram bot

## Data storage

All records are stored in configured `data_dir` (default: `data`) and grouped per Telegram user/chat id.

Structure:

- `data/<user_id>/glucose.csv` — glucose measurements
- `data/<user_id>/weight.csv` — weight measurements
- `data/<user_id>/medications.txt` — medication names (one per line)
- `data/<user_id>/medication_log.csv` — medication usage events

## Requirements

- Rust toolchain (stable)
- Telegram bot token from BotFather
- Your Telegram chat id

## Configuration

Create `pdd-bot/config.toml`:

```toml
tg_bot_token = "<YOUR_TELEGRAM_BOT_TOKEN>"
tg_chat_id = ["<YOUR_CHAT_ID>"]
data_dir = "data"
input_timezone = "Europe/Kyiv"
glucose_after_meal_reminder_minutes = 150
glucose_after_meal_reminder_count = 3
glucose_after_meal_reminder_interval_minutes = 15
```

Fields:

- `tg_bot_token` is required. Use the Telegram bot token from BotFather.
- `tg_chat_id` is required. It is a list of allowed Telegram chat IDs.
- `data_dir` is optional. It defaults to `data`.
- `input_timezone` is optional. It defaults to `UTC` and is used to interpret manually entered date/time without timezone.
- `glucose_after_meal_reminder_minutes` is optional. It defaults to `150`; set it to `0` to disable after-meal reminders.
- `glucose_after_meal_reminder_count` is optional. It defaults to `3`; set it to `0` to disable after-meal reminders.
- `glucose_after_meal_reminder_interval_minutes` is optional. It defaults to `15` and controls the delay between repeated reminders.

Reminder behavior:

- Adding glucose before meal schedules after-meal reminders for that chat.
- Adding glucose after meal cancels pending after-meal reminders for that chat.
- The first reminder is sent after `glucose_after_meal_reminder_minutes`.
- Additional reminders are sent every `glucose_after_meal_reminder_interval_minutes` until `glucose_after_meal_reminder_count` reminders have been sent.

Notes:

- Timestamps are stored in UTC in CSV files.
- Replace token/id values with your own.

## Build

From repository root:

```bash
cd pdd-bot
cargo build
```

Release build:

```bash
cd pdd-bot
cargo build --release
```

## Run

With default config (`config.toml`):

```bash
cd pdd-bot
cargo run
```

With custom config path:

```bash
cd pdd-bot
cargo run -- --config path/to/config.toml
```

Config check command:

```bash
cd pdd-bot
cargo run -- check-config --config config.toml
```

## Telegram usage

### Menu-based input

1. Send `/start` or `/menu`.
2. Use buttons:
	 - `🩸 Glucose: Before meal`
	 - `🩸 Glucose: After meal`
	 - `⚖️ Weight`
	 - medication buttons (`💊 ...`)

For glucose button flow, send:

```text
<value> [date time] [@note]
```

Examples:

- `5.8`
- `7.2 2/1 11:00`
- `6.4 2024/2/1 09:05 @after oatmeal + tea`

For weight button flow, send value only:

- `78.4`

### Commands

- `/help` — show help
- `/menu` — show buttons
- `/addmed <name>` — add medication button
- `/addgb <value> [date time] [@note]` — add glucose before meal
- `/addga <value> [date time] [@note]` — add glucose after meal

Aliases:

- `/add_glucose_before`
- `/add_glucose_after`

Examples:

```text
/addmed Metformin
/addgb 5.6 @fasting
/addga 7.8 2/1 10:30 @after breakfast
/addgb 6.1 2024/2/1 9:05 @before gym
```

## Supported date/time formats

Accepted date/time part:

- `MM/DD hh:mm`
- `YY/MM/DD hh:mm`
- `YYYY/MM/DD hh:mm`

Rules:

- month/day/hour/minute can be 1 or 2 digits
- year can be 2 or 4 digits
- if year is omitted, current year is used
- `-` and `.` are also accepted as date separators

Examples:

- `2/1 9:05`
- `02/01 09:05`
- `24/2/1 9:05`
- `2024/2/1 9:05`

## Notes format

Use `@` to start note text:

```text
... @your note text with spaces and symbols !?+#
```

Everything after `@` is saved as note.

## Troubleshooting

- Bot does not reply:
	- verify `tg_bot_token`
	- verify your chat id is included in `tg_chat_id`
	- send `/start` to bot in Telegram first
- `cargo run` exits with error:
	- verify `config.toml` exists
	- verify token/chat id format
