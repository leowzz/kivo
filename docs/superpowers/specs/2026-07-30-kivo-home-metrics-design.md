# Kivo Home Metrics Design

## Scope

Add a Home view to Kivo. It shows current device status, today's button-use
metrics, a seven-day heat map, and a readable activity log. Existing behavior,
hardware, and layout editors remain unchanged.

## User Interface

The sidebar gains Home as the first navigation item. The Home view contains:

- device status: connection state, serial port, and last successful activity;
- today's summary: total presses, active-button count, and connected duration;
- button performance: most-used button, least-recently-used button, and a
  seven-day per-button heat map;
- an activity-log sidebar, newest first, for device connections, button presses,
  and configuration changes.

The default metric window is today. Seven-day heat data is shown alongside it.

## Storage

Use a SQLite database in the Tauri application data directory. Add `rusqlite`
without a new service or frontend persistence layer.

`button_metrics` stores one row per model and button with total press count and
last-pressed timestamp. `button_metric_days` stores one row per model, button,
and local calendar date with a daily press count. `activity_logs` stores a
timestamp, event type, and human-readable message.

Keep the newest 500 activity-log rows. Each append removes older rows in the
same transaction. Metrics are keyed by model and button identifier so layout
statistics remain distinct between models.

## Data Flow

When a physical button press is accepted by the existing serial runtime, Kivo
updates total and daily metrics, appends a log entry, then emits an incremental
runtime event. Configuration writes and connection-state changes append their
own log entries.

The frontend loads a Home snapshot through a Tauri command. It applies runtime
events locally to keep the view current without polling. The snapshot includes
connection state, summary values, per-button seven-day data, and the recent
activity log.

## Failure Behavior

Metric persistence never blocks the configured button action. A SQLite failure
is surfaced through the existing runtime-error mechanism and Home renders its
metric area as unavailable. The configuration workflow remains usable.

## Verification

Add a focused Rust test for database creation, press recording, date aggregation,
log retention, and snapshot reads. Add a React render test for the Home snapshot
and an incremental press update.
