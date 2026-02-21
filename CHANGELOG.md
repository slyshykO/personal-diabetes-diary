# Changelog

All notable changes to this project are documented in this file.

## 0.1.0 [2026-02-21]

### Added
- Telegram reply-keyboard menu for faster diary input.
- Glucose logging with tags: before meal / after meal.
- Weight logging.
- Medication management:
  - Add medication via `/addmed <name>`.
  - Dynamic medication buttons in menu.
  - Medication usage logging to CSV.
- Glucose direct commands:
  - `/addgb <value> [date time] [@note]`
  - `/addga <value> [date time] [@note]`
- Flexible date/time parser for glucose input:
  - `MM/DD hh:mm`
  - `YY/MM/DD hh:mm`
  - `YYYY/MM/DD hh:mm`
  - 1–2 digit month/day/hour/minute, 2–4 digit year
  - Current year is used if omitted.
- Glucose notes support with `@note` syntax (spaces/symbols supported).
- `/help` command with usage examples.
- Plain-text data security warning in `/help` and README.

### Changed
- Menu made more compact:
  - Glucose buttons on one row.
  - Weight + Show menu on one row.
  - Medication buttons rendered in compact rows.
- Data storage moved to per-user folders:
  - `data/<user_id>/glucose.csv`
  - `data/<user_id>/weight.csv`
  - `data/<user_id>/medications.txt`
  - `data/<user_id>/medication_log.csv`
- Timestamp handling standardized:
  - All stored timestamps are normalized to UTC.
  - Added configurable `input_timezone` for interpreting manually entered date/time.

### Documentation
- Added comprehensive repository README with setup, build, run, and usage.
- Added Quick Start section.
- Documented data security warning and UTC/timezone behavior.
- Reduced duplicate docs in `pdd-bot/README.md` and pointed to root README.
