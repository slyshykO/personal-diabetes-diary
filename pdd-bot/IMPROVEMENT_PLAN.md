# Project review and improvement plan

Reviewed: 2026-10-06

The Telegram bot supports glucose, weight, medication logging, reminders, and
HTML reports. The Rust modules are reasonably separated. The web app is a
scaffold with a counter, and the HTTP API currently exposes only health checks.

Fix data correctness and restart reliability first, then improve daily use and
build the web diary. Work through the tasks below one at a time. Each task should
include the implementation, relevant verification, and an update to this plan.

## 1. Data correctness and reliability

### 1. Fix reports for multiline notes

- [ ] Complete

**Finding:** Notes can contain newlines, but the report reader parses CSV one
physical line at a time. A two-line note was confirmed to make report generation
fail with `unterminated quoted CSV field`.

**Source:** [Report reader](src/reports/mod.rs), [CSV storage](src/data/mod.rs).

**Work:** Use a CSV reader that supports multiline records. Keep existing diary
files readable and ensure writing and reading agree on quoting rules.

**Acceptance checks:** Existing records and notes containing commas, quotes,
newlines, and Unicode remain readable. A multiline note no longer prevents
report generation. Add regression coverage for these cases.

### 2. Use the configured timezone in reports

- [ ] Complete

**Finding:** Input uses `input_timezone`, but reports convert timestamps to the
server's `Local` timezone. Different server and user timezones can put entries
under the wrong day or meal.

**Source:** [Report generation](src/reports/mod.rs),
[report caller](src/tgbot.rs).

**Work:** Pass the configured timezone into report generation. Continue storing
timestamps in UTC and use the configured timezone for report dates and times.

**Acceptance checks:** Report grouping is independent of the server timezone.
Cover a local midnight boundary and daylight saving transitions with tests.

### 3. Make service installation survive reboot

- [ ] Complete

**Finding:** The installer writes the service unit into `/run/systemd/system`,
which is temporary, and starts the service without enabling startup.

**Source:** [Linux installer](src/install/install_linux.rs),
[service unit](systemd/pdd.service).

**Work:** Install the unit persistently under `/etc/systemd/system` and enable
startup. Handle an existing installation in `/run` and align uninstall behavior
with the new location. Preserve the existing configuration and diary data during
an upgrade.

**Acceptance checks:** Installation and repeated installation succeed; the
service is enabled and can start after reboot. Verify upgrade and uninstall
behavior in an isolated systemd environment and document any checks that cannot
be run locally.

### 4. Persist reminders and handle backdated entries

- [ ] Complete

**Finding:** Reminders exist only as in-memory tasks and disappear on restart.
A historical before-meal entry schedules a reminder relative to now, while a
historical after-meal entry cancels the current reminder.

**Source:** [Reminder scheduling](src/tgbot.rs).

**Work:** Define how entry timestamps affect scheduling and cancellation. Persist
deadlines and reminder progress, restore them on startup, and define what happens
to reminders that became overdue while the service was stopped.

**Acceptance checks:** Restarting preserves pending reminders without duplicating
them. Historical entries do not unexpectedly create or cancel current reminders.
Cover replacement, cancellation, overdue reminders, and disabled reminders with
controlled-time tests.

### 5. Validate measurement values

- [ ] Complete

**Finding:** The shared numeric parser accepts `NaN`, infinity, negative values,
and zero. These inputs were confirmed during review.

**Source:** [Input parsing](src/tgbot.rs), [storage functions](src/data/mod.rs).

**Work:** Require finite, positive glucose and weight values. Preserve decimal
comma support, display units in prompts, and return useful validation errors.
Keep validation reusable by future HTTP entry points.

**Acceptance checks:** Valid dot and comma decimals are accepted. Non-finite,
zero, negative, and malformed values are rejected without writing records.

### 6. Make HTTP access restrictions explicit and effective

- [ ] Complete

**Finding:** `html_config.allow` is stored but never checked; the validation
middleware is commented out. The documented Caddy setup provides authentication,
but direct backend access bypasses it. The default listen address is all IPv4
interfaces when HTTP is enabled.

**Source:** [HTTP server](src/http/mod.rs), [configuration](src/args.rs),
[Caddy setup](caddy/README.md).

**Work:** Enforce the allow setting or remove it and document the supported access
model. Default the backend to loopback. Define proxy trust explicitly if client
IP filtering is retained.

**Acceptance checks:** Configuration accurately describes enforced behavior.
Verify allowed and rejected access as applicable, including requests through
Caddy. Establish authorization before adding diary data endpoints.

### 7. Wait for graceful shutdown

- [ ] Complete

**Finding:** Main sends shutdown signals and returns without waiting for task
completion. The runtime can exit before HTTP requests or bot handlers finish.

**Source:** [Shutdown flow](src/main.rs), [task supervision](src/state.rs),
[bot lifecycle](src/tgbot.rs), [HTTP lifecycle](src/http/mod.rs).

**Work:** Retain task handles, stop accepting new work, and await running tasks
with a bounded timeout. Handle partial startup failures and reminder shutdown.

**Acceptance checks:** In-flight work can finish during normal shutdown; a stuck
task cannot block exit indefinitely. Task failures remain visible and partial
startup failures clean up services already started.

## 2. Build, tests, and project documentation

### 8. Establish a reproducible build and useful CI

- [ ] Complete

**Findings:** There is no root README or checked-in CI workflow. `Cargo.lock` is
ignored. Rust embeds generated frontend assets, so a clean checkout needs the
frontend built first. `npm test` is a placeholder, and the F# test project contains
only `Assert.True(true)`.

**Work:**

- Document prerequisites, configuration, development commands, build order,
  deployment, and test commands in a root README.
- Commit the application Cargo lockfile and use locked dependency installation
  in CI.
- Add CI that builds the frontend before compiling and testing Rust.
- Establish a working frontend test command with meaningful coverage as diary
  behavior is added; ensure test execution is visible.
- Keep regression tests for tasks 1–7 alongside those fixes.

**Acceptance checks:** A clean checkout builds using documented commands. CI
reports actual test execution and fails on compilation or test errors.

## 3. Daily diary usability

### 9. Add cancellation and recent history

- [ ] Complete

**Work:** Add `/cancel` to clear pending input and a way to inspect recent glucose,
weight, and medication entries. Show timestamps, units, and relevant notes.

**Acceptance checks:** Cancellation cannot accidentally save an entry. Recent
history is limited to the requesting chat and uses the configured timezone.

### 10. Add correction and undo

- [ ] Complete

**Work:** Choose a stable way to identify entries, then support correcting a
mistake and undoing an accidental entry. Define how corrections affect reminders
and preserve enough history to recover from an erroneous correction.

**Acceptance checks:** Only the intended entry changes. Another chat's records
cannot be modified. Corrections remain consistent in history and reports.

### 11. Add backup, export, and restore instructions

- [ ] Complete

**Work:** Provide export and backup procedures for diary data and persisted
reminders. Document restoration and how configuration is handled.

**Acceptance checks:** Restore a backup into an isolated data directory and
verify entries, notes, timestamps, medication lists, and pending reminder state.
Exporting one chat must not include another chat's data.

## 4. Web diary

### 12. Define web identity and diary authorization

- [ ] Complete

**Work:** Decide how authenticated web access maps to Telegram chat data and
implement that policy before exposing diary endpoints. The existing Caddy login
does not itself define which chat's records a user may access.

**Acceptance checks:** Unauthorized requests cannot read diary data, and identity
mapping prevents access to another user's records.

### 13. Replace the counter with a diary dashboard

- [ ] Complete

**Source:** [Frontend state](fastoch-app/State.fs),
[frontend views](fastoch-app/View.fs), [HTTP API](src/http/mod.rs).

**Work:** Add authenticated history endpoints and a dashboard with date filters,
notes, and glucose and weight charts. Reuse the validated storage and timezone
behavior established earlier. Include loading, empty, and error states.

**Acceptance checks:** Displayed records match stored data and exported reports.
Date filters respect the configured timezone. The interface works on mobile and
does not expose another chat's data.

## Review verification baseline

- `cargo test --offline`: four Rust tests passed.
- Frontend production bundle built successfully using cached Fable output; a
  clean Fable compilation was not verified.
- The F# test command exited successfully without showing test execution. Its
  only source test was `Assert.True(true)`.
- A temporary harness reproduced multiline-note report failure and acceptance
  of `NaN`, infinity, negative values, and zero by the numeric parser.
- Live Telegram behavior, Caddy deployment, service installation, and reboot
  behavior were not exercised during review.
- No tracked source files were changed during the review.

## Progress notes

Record completed work, verification results, and any remaining limitations here
as each task is finished.

- 2026-10-06: Review converted into this plan. Implementation has not started.
