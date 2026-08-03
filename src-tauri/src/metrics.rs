use crate::hardware::DeviceId;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Mutex,
};

const METRICS_SCHEMA_VERSION: i64 = 2;
const ACTIVITY_LOG_LIMIT: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricAttribution {
    pub device_id: DeviceId,
    pub device_name: String,
    pub device_profile_id: String,
    pub hardware_profile_id: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeMetricsSnapshot {
    pub total_presses: u64,
    pub today_presses: u64,
    pub active_button_count: u64,
    pub top_button: Option<ButtonMetric>,
    pub heatmap: Vec<ButtonDayMetric>,
    pub logs: Vec<ActivityLog>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonMetric {
    pub button_id: String,
    pub presses: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonDayMetric {
    pub button_id: String,
    pub day: String,
    pub presses: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLog {
    pub timestamp_ms: u64,
    pub kind: String,
    pub message: String,
    pub device_id: DeviceId,
    pub device_name: String,
    pub device_profile_id: String,
    pub hardware_profile_id: String,
    pub button_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ActivityLogBackup {
    pub occurred_at_ms: u64,
    pub kind: String,
    pub message: String,
    pub device_id: DeviceId,
    pub device_name: String,
    pub device_profile_id: String,
    pub hardware_profile_id: String,
    pub button_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonMetricAggregate {
    pub device_profile_id: String,
    pub device_id: DeviceId,
    pub button_id: String,
    pub total_presses: u64,
    pub last_pressed_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonMetricDayAggregate {
    pub device_profile_id: String,
    pub device_id: DeviceId,
    pub button_id: String,
    pub day: String,
    pub presses: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct MetricsBackup {
    pub button_metrics: Vec<ButtonMetricAggregate>,
    pub button_metric_days: Vec<ButtonMetricDayAggregate>,
    pub activity_logs: Vec<ActivityLogBackup>,
}

impl MetricsBackup {
    pub fn validate(&self) -> Result<(), rusqlite::Error> {
        if self.activity_logs.len() > ACTIVITY_LOG_LIMIT {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut aggregate_keys = BTreeSet::new();
        let mut aggregate_totals = BTreeMap::new();
        for metric in &self.button_metrics {
            let key = (
                metric.device_profile_id.as_str(),
                metric.device_id.as_str(),
                metric.button_id.as_str(),
            );
            if metric.device_profile_id.is_empty()
                || metric.button_id.is_empty()
                || metric.total_presses == 0
                || !aggregate_keys.insert(key)
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            aggregate_totals.insert(key, metric.total_presses);
        }
        let mut day_keys = BTreeSet::new();
        let mut day_totals = BTreeMap::new();
        for metric in &self.button_metric_days {
            let aggregate_key = (
                metric.device_profile_id.as_str(),
                metric.device_id.as_str(),
                metric.button_id.as_str(),
            );
            let total = day_totals.entry(aggregate_key).or_insert(0_u64);
            *total = total
                .checked_add(metric.presses)
                .ok_or(rusqlite::Error::InvalidQuery)?;
            if !valid_day(&metric.day)
                || metric.presses == 0
                || !aggregate_keys.contains(&aggregate_key)
                || *total > aggregate_totals[&aggregate_key]
                || !day_keys.insert((aggregate_key, metric.day.as_str()))
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
        }
        if aggregate_totals
            .iter()
            .any(|(key, total)| day_totals.get(key) != Some(total))
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if self.activity_logs.iter().any(|log| {
            log.kind.is_empty()
                || log.device_name.trim().is_empty()
                || log.device_profile_id.is_empty()
                || log.hardware_profile_id.is_empty()
                || log.button_id.as_ref().is_some_and(|button_id| {
                    !aggregate_keys.contains(&(
                        log.device_profile_id.as_str(),
                        log.device_id.as_str(),
                        button_id.as_str(),
                    ))
                })
        }) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    }
}

fn valid_day(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u16>().unwrap_or_default();
    let month = value[5..7].parse::<u8>().unwrap_or_default();
    let day = value[8..10].parse::<u8>().unwrap_or_default();
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day)
}

pub struct MetricsStore {
    connection: Mutex<Option<Connection>>,
}

impl MetricsStore {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        Ok(Self {
            connection: Mutex::new(Some(open_connection(path)?)),
        })
    }

    pub fn close(&self) {
        self.connection
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
    }

    pub fn reopen(&self, path: &Path) -> Result<(), rusqlite::Error> {
        let connection = open_connection(path)?;
        *self
            .connection
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(connection);
        Ok(())
    }

    pub fn record_button_press(
        &self,
        attribution: &MetricAttribution,
        button_id: &str,
        timestamp_ms: u64,
    ) -> Result<(), rusqlite::Error> {
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let connection = connection.as_mut().ok_or(rusqlite::Error::InvalidQuery)?;
        let timestamp_ms = integer(timestamp_ms)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "
            INSERT INTO button_metrics (
                device_profile_id, device_id, button_id, total_presses, last_pressed_at_ms
            ) VALUES (?1, ?2, ?3, 1, ?4)
            ON CONFLICT(device_profile_id, device_id, button_id) DO UPDATE SET
                total_presses = total_presses + 1,
                last_pressed_at_ms = excluded.last_pressed_at_ms
            ",
            params![
                attribution.device_profile_id,
                attribution.device_id.as_str(),
                button_id,
                timestamp_ms
            ],
        )?;
        transaction.execute(
            "
            INSERT INTO button_metric_days (
                device_profile_id, device_id, button_id, day, presses
            ) VALUES (?1, ?2, ?3, strftime('%Y-%m-%d', ?4 / 1000, 'unixepoch', 'localtime'), 1)
            ON CONFLICT(device_profile_id, device_id, button_id, day) DO UPDATE SET
                presses = presses + 1
            ",
            params![
                attribution.device_profile_id,
                attribution.device_id.as_str(),
                button_id,
                timestamp_ms
            ],
        )?;
        transaction.execute(
            "
            INSERT INTO activity_logs (
                occurred_at_ms, kind, message, device_id, device_name,
                device_profile_id, hardware_profile_id, button_id
            ) VALUES (?1, 'button', ?2, ?3, ?4, ?5, ?6, ?7)
            ",
            params![
                timestamp_ms,
                format!("{button_id} pressed"),
                attribution.device_id.as_str(),
                attribution.device_name,
                attribution.device_profile_id,
                attribution.hardware_profile_id,
                button_id
            ],
        )?;
        trim_activity_logs(&transaction)?;
        transaction.commit()
    }

    pub fn home_snapshot(
        &self,
        device_profile_id: &str,
        device_id: Option<&DeviceId>,
        now_ms: u64,
    ) -> Result<HomeMetricsSnapshot, rusqlite::Error> {
        self.snapshot(Some(device_profile_id), device_id, now_ms)
    }

    pub fn device_snapshot(
        &self,
        device_id: &DeviceId,
        now_ms: u64,
    ) -> Result<HomeMetricsSnapshot, rusqlite::Error> {
        self.snapshot(None, Some(device_id), now_ms)
    }

    fn snapshot(
        &self,
        device_profile_id: Option<&str>,
        device_id: Option<&DeviceId>,
        now_ms: u64,
    ) -> Result<HomeMetricsSnapshot, rusqlite::Error> {
        let connection = self
            .connection
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let connection = connection.as_ref().ok_or(rusqlite::Error::InvalidQuery)?;
        let now_ms = integer(now_ms)?;
        let device_id = device_id.map(DeviceId::as_str);
        let total_presses = connection.query_row(
            "
            SELECT COALESCE(SUM(total_presses), 0)
            FROM button_metrics
            WHERE (?1 IS NULL OR device_profile_id = ?1)
              AND (?2 IS NULL OR device_id = ?2)
            ",
            params![device_profile_id, device_id],
            |row| nonnegative(row, 0),
        )?;
        let today_presses = connection.query_row(
            "
            SELECT COALESCE(SUM(presses), 0)
            FROM button_metric_days
            WHERE (?1 IS NULL OR device_profile_id = ?1)
              AND (?2 IS NULL OR device_id = ?2)
              AND day = strftime('%Y-%m-%d', ?3 / 1000, 'unixepoch', 'localtime')
            ",
            params![device_profile_id, device_id, now_ms],
            |row| nonnegative(row, 0),
        )?;
        let active_button_count = connection.query_row(
            "
            SELECT COUNT(DISTINCT button_id)
            FROM button_metric_days
            WHERE (?1 IS NULL OR device_profile_id = ?1)
              AND (?2 IS NULL OR device_id = ?2)
              AND day = strftime('%Y-%m-%d', ?3 / 1000, 'unixepoch', 'localtime')
            ",
            params![device_profile_id, device_id, now_ms],
            |row| nonnegative(row, 0),
        )?;
        let top_button = connection
            .query_row(
                "
                SELECT button_id, SUM(total_presses) AS presses
                FROM button_metrics
                WHERE (?1 IS NULL OR device_profile_id = ?1)
                  AND (?2 IS NULL OR device_id = ?2)
                GROUP BY button_id
                ORDER BY presses DESC, button_id ASC
                LIMIT 1
                ",
                params![device_profile_id, device_id],
                |row| {
                    Ok(ButtonMetric {
                        button_id: row.get(0)?,
                        presses: nonnegative(row, 1)?,
                    })
                },
            )
            .optional()?;
        let mut statement = connection.prepare(
            "
            SELECT button_id, day, SUM(presses)
            FROM button_metric_days
            WHERE (?1 IS NULL OR device_profile_id = ?1)
              AND (?2 IS NULL OR device_id = ?2)
              AND day >= date(?3 / 1000, 'unixepoch', 'localtime', '-6 days')
              AND day <= date(?3 / 1000, 'unixepoch', 'localtime')
            GROUP BY button_id, day
            ORDER BY day ASC, button_id ASC
            ",
        )?;
        let heatmap = statement
            .query_map(params![device_profile_id, device_id, now_ms], |row| {
                Ok(ButtonDayMetric {
                    button_id: row.get(0)?,
                    day: row.get(1)?,
                    presses: nonnegative(row, 2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(HomeMetricsSnapshot {
            total_presses,
            today_presses,
            active_button_count,
            top_button,
            heatmap,
            logs: logs(connection, device_profile_id, device_id)?,
        })
    }

    pub fn backup(&self) -> Result<MetricsBackup, rusqlite::Error> {
        let connection = self
            .connection
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let connection = connection.as_ref().ok_or(rusqlite::Error::InvalidQuery)?;
        Ok(MetricsBackup {
            button_metrics: button_metric_aggregates(connection)?,
            button_metric_days: button_metric_day_aggregates(connection)?,
            activity_logs: all_logs(connection)?,
        })
    }

    pub fn replace_from_backup(&self, backup: &MetricsBackup) -> Result<(), rusqlite::Error> {
        backup.validate()?;
        let mut connection = self
            .connection
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let connection = connection.as_mut().ok_or(rusqlite::Error::InvalidQuery)?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM button_metrics", [])?;
        transaction.execute("DELETE FROM button_metric_days", [])?;
        transaction.execute("DELETE FROM activity_logs", [])?;
        for metric in &backup.button_metrics {
            transaction.execute(
                "
                INSERT INTO button_metrics (
                    device_profile_id, device_id, button_id, total_presses, last_pressed_at_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    metric.device_profile_id,
                    metric.device_id.as_str(),
                    metric.button_id,
                    integer(metric.total_presses)?,
                    integer(metric.last_pressed_at_ms)?
                ],
            )?;
        }
        for metric in &backup.button_metric_days {
            transaction.execute(
                "
                INSERT INTO button_metric_days (
                    device_profile_id, device_id, button_id, day, presses
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    metric.device_profile_id,
                    metric.device_id.as_str(),
                    metric.button_id,
                    metric.day,
                    integer(metric.presses)?
                ],
            )?;
        }
        for log in backup.activity_logs.iter().rev() {
            transaction.execute(
                "
                INSERT INTO activity_logs (
                    occurred_at_ms, kind, message, device_id, device_name,
                    device_profile_id, hardware_profile_id, button_id
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ",
                params![
                    integer(log.occurred_at_ms)?,
                    log.kind,
                    log.message,
                    log.device_id.as_str(),
                    log.device_name,
                    log.device_profile_id,
                    log.hardware_profile_id,
                    log.button_id
                ],
            )?;
        }
        transaction.commit()
    }
}

fn open_connection(path: &Path) -> Result<Connection, rusqlite::Error> {
    let connection = Connection::open(path)?;
    let version = connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))?;
    if version > METRICS_SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if has_legacy_metrics_schema(&connection)? {
        connection.execute_batch(
            "
            DROP TABLE IF EXISTS button_metrics;
            DROP TABLE IF EXISTS button_metric_days;
            DROP TABLE IF EXISTS activity_logs;
            ",
        )?;
    }
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS button_metrics (
            device_profile_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            button_id TEXT NOT NULL,
            total_presses INTEGER NOT NULL,
            last_pressed_at_ms INTEGER NOT NULL,
            PRIMARY KEY (device_profile_id, device_id, button_id)
        );
        CREATE TABLE IF NOT EXISTS button_metric_days (
            device_profile_id TEXT NOT NULL,
            device_id TEXT NOT NULL,
            button_id TEXT NOT NULL,
            day TEXT NOT NULL,
            presses INTEGER NOT NULL,
            PRIMARY KEY (device_profile_id, device_id, button_id, day)
        );
        CREATE TABLE IF NOT EXISTS activity_logs (
            id INTEGER PRIMARY KEY,
            occurred_at_ms INTEGER NOT NULL,
            kind TEXT NOT NULL,
            message TEXT NOT NULL,
            device_id TEXT NOT NULL,
            device_name TEXT NOT NULL,
            device_profile_id TEXT NOT NULL,
            hardware_profile_id TEXT NOT NULL,
            button_id TEXT
        );
        CREATE INDEX IF NOT EXISTS activity_logs_profile_device
            ON activity_logs(device_profile_id, device_id, id DESC);
        PRAGMA user_version = 2;
        ",
    )?;
    Ok(connection)
}

fn has_legacy_metrics_schema(connection: &Connection) -> Result<bool, rusqlite::Error> {
    let exists = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'button_metrics')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !exists {
        return Ok(false);
    }
    let mut statement = connection.prepare("PRAGMA table_info(button_metrics)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(!columns.iter().any(|column| column == "device_profile_id"))
}

fn trim_activity_logs(transaction: &rusqlite::Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.execute(
        "DELETE FROM activity_logs WHERE id NOT IN (SELECT id FROM activity_logs ORDER BY id DESC LIMIT 500)",
        [],
    )?;
    Ok(())
}

fn logs(
    connection: &Connection,
    device_profile_id: Option<&str>,
    device_id: Option<&str>,
) -> Result<Vec<ActivityLog>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "
        SELECT occurred_at_ms, kind, message, device_id, device_name,
               device_profile_id, hardware_profile_id, button_id
        FROM activity_logs
        WHERE (?1 IS NULL OR device_profile_id = ?1)
          AND (?2 IS NULL OR device_id = ?2)
        ORDER BY id DESC
        LIMIT 500
        ",
    )?;
    let rows = statement
        .query_map(params![device_profile_id, device_id], activity_log_backup)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(ActivityLog::from).collect())
}

fn all_logs(connection: &Connection) -> Result<Vec<ActivityLogBackup>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "
        SELECT occurred_at_ms, kind, message, device_id, device_name,
               device_profile_id, hardware_profile_id, button_id
        FROM activity_logs
        ORDER BY id DESC
        LIMIT 500
        ",
    )?;
    statement.query_map([], activity_log_backup)?.collect()
}

fn activity_log_backup(row: &rusqlite::Row<'_>) -> Result<ActivityLogBackup, rusqlite::Error> {
    let raw_device_id = row.get::<_, String>(3)?;
    Ok(ActivityLogBackup {
        occurred_at_ms: nonnegative(row, 0)?,
        kind: row.get(1)?,
        message: row.get(2)?,
        device_id: DeviceId::parse(&raw_device_id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        device_name: row.get(4)?,
        device_profile_id: row.get(5)?,
        hardware_profile_id: row.get(6)?,
        button_id: row.get(7)?,
    })
}

impl From<ActivityLogBackup> for ActivityLog {
    fn from(value: ActivityLogBackup) -> Self {
        Self {
            timestamp_ms: value.occurred_at_ms,
            kind: value.kind,
            message: value.message,
            device_id: value.device_id,
            device_name: value.device_name,
            device_profile_id: value.device_profile_id,
            hardware_profile_id: value.hardware_profile_id,
            button_id: value.button_id,
        }
    }
}

fn button_metric_aggregates(
    connection: &Connection,
) -> Result<Vec<ButtonMetricAggregate>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "
        SELECT device_profile_id, device_id, button_id, total_presses, last_pressed_at_ms
        FROM button_metrics
        ORDER BY device_profile_id, device_id, button_id
        ",
    )?;
    statement
        .query_map([], |row| {
            let raw_device_id = row.get::<_, String>(1)?;
            Ok(ButtonMetricAggregate {
                device_profile_id: row.get(0)?,
                device_id: parse_device_id(raw_device_id, 1)?,
                button_id: row.get(2)?,
                total_presses: nonnegative(row, 3)?,
                last_pressed_at_ms: nonnegative(row, 4)?,
            })
        })?
        .collect()
}

fn button_metric_day_aggregates(
    connection: &Connection,
) -> Result<Vec<ButtonMetricDayAggregate>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "
        SELECT device_profile_id, device_id, button_id, day, presses
        FROM button_metric_days
        ORDER BY device_profile_id, device_id, button_id, day
        ",
    )?;
    statement
        .query_map([], |row| {
            let raw_device_id = row.get::<_, String>(1)?;
            Ok(ButtonMetricDayAggregate {
                device_profile_id: row.get(0)?,
                device_id: parse_device_id(raw_device_id, 1)?,
                button_id: row.get(2)?,
                day: row.get(3)?,
                presses: nonnegative(row, 4)?,
            })
        })?
        .collect()
}

fn parse_device_id(value: String, column: usize) -> Result<DeviceId, rusqlite::Error> {
    DeviceId::parse(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn integer(value: u64) -> Result<i64, rusqlite::Error> {
    i64::try_from(value).map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn nonnegative(row: &rusqlite::Row<'_>, column: usize) -> Result<u64, rusqlite::Error> {
    let value = row.get::<_, i64>(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}

#[cfg(test)]
mod tests {
    use super::{MetricAttribution, MetricsStore};
    use crate::hardware::DeviceId;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "kivo-metrics-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn attribution_is_immutable_across_reassignment_and_forgetting() {
        let directory = TestDirectory::new();
        let store = MetricsStore::open(&directory.0.join("metrics.sqlite3")).unwrap();
        let yesterday = 1_720_000_000_000;
        let today = yesterday + 86_400_000;
        let device_a = DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap();
        let device_b = DeviceId::new("luatos-esp32s3-aio", "BBBBBBBBBBBB").unwrap();
        let original_a = MetricAttribution {
            device_id: device_a.clone(),
            device_name: "Desk A".into(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        let original_b = MetricAttribution {
            device_id: device_b.clone(),
            device_name: "Desk B".into(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        store
            .record_button_press(&original_a, "ONE", yesterday)
            .unwrap();
        store
            .record_button_press(&original_a, "ONE", today)
            .unwrap();
        store
            .record_button_press(&original_b, "TWO", today)
            .unwrap();

        let reassigned_a = MetricAttribution {
            device_name: "Renamed A".into(),
            device_profile_id: "console".into(),
            hardware_profile_id: "esp-alternate".into(),
            ..original_a
        };
        store
            .record_button_press(&reassigned_a, "THREE", today + 1)
            .unwrap();

        let profile_snapshot = store.home_snapshot("phone", None, today).unwrap();
        assert_eq!(profile_snapshot.total_presses, 3);
        assert_eq!(profile_snapshot.today_presses, 2);
        assert_eq!(profile_snapshot.active_button_count, 2);
        assert_eq!(
            profile_snapshot
                .top_button
                .as_ref()
                .map(|button| button.button_id.as_str()),
            Some("ONE")
        );
        assert_eq!(profile_snapshot.logs.len(), 3);
        assert_eq!(profile_snapshot.logs[0].device_name, "Desk B");
        assert_eq!(profile_snapshot.logs[1].device_name, "Desk A");
        assert_eq!(profile_snapshot.logs[1].device_profile_id, "phone");
        assert_eq!(profile_snapshot.logs[1].hardware_profile_id, "esp-primary");

        let device_snapshot = store
            .home_snapshot("phone", Some(&device_a), today)
            .unwrap();
        assert_eq!(device_snapshot.total_presses, 2);
        assert_eq!(device_snapshot.today_presses, 1);
        assert!(
            device_snapshot
                .logs
                .iter()
                .all(|log| log.device_id == device_a)
        );

        let reassigned_snapshot = store
            .home_snapshot("console", Some(&device_a), today)
            .unwrap();
        assert_eq!(reassigned_snapshot.total_presses, 1);
        assert_eq!(reassigned_snapshot.logs[0].device_name, "Renamed A");

        let complete_device_snapshot = store.device_snapshot(&device_a, today + 1).unwrap();
        assert_eq!(complete_device_snapshot.total_presses, 3);
        assert_eq!(complete_device_snapshot.today_presses, 2);
        assert_eq!(complete_device_snapshot.active_button_count, 2);
        assert_eq!(
            complete_device_snapshot
                .top_button
                .as_ref()
                .map(|button| (button.button_id.as_str(), button.presses)),
            Some(("ONE", 2))
        );
        assert_eq!(complete_device_snapshot.logs.len(), 3);
        assert_eq!(complete_device_snapshot.logs[0].device_name, "Renamed A");
        assert_eq!(
            complete_device_snapshot.logs[0].device_profile_id,
            "console"
        );
        assert_eq!(
            complete_device_snapshot.logs[0].hardware_profile_id,
            "esp-alternate"
        );
        assert_eq!(complete_device_snapshot.logs[2].device_name, "Desk A");
        assert_eq!(complete_device_snapshot.logs[2].device_profile_id, "phone");
        assert_eq!(
            complete_device_snapshot.logs[2].hardware_profile_id,
            "esp-primary"
        );
    }

    #[test]
    fn backup_round_trips_aggregates_and_only_the_newest_500_activities() {
        let directory = TestDirectory::new();
        let source = MetricsStore::open(&directory.0.join("source.sqlite3")).unwrap();
        let device = DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap();
        let attribution = MetricAttribution {
            device_id: device.clone(),
            device_name: "Desk".into(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        let today = 1_720_086_400_000;
        for number in 0..501 {
            source
                .record_button_press(&attribution, "ONE", today + number)
                .unwrap();
        }

        let backup = source.backup().unwrap();
        assert_eq!(backup.button_metrics.len(), 1);
        assert_eq!(backup.button_metrics[0].total_presses, 501);
        assert_eq!(backup.button_metric_days.len(), 1);
        assert_eq!(backup.button_metric_days[0].presses, 501);
        assert_eq!(backup.activity_logs.len(), 500);
        assert_eq!(backup.activity_logs[0].occurred_at_ms, today + 500);
        assert_eq!(backup.activity_logs[499].occurred_at_ms, today + 1);

        let restored = MetricsStore::open(&directory.0.join("restored.sqlite3")).unwrap();
        restored.replace_from_backup(&backup).unwrap();
        assert_eq!(restored.backup().unwrap(), backup);
        let snapshot = restored
            .home_snapshot("phone", Some(&device), today + 500)
            .unwrap();
        assert_eq!(snapshot.total_presses, 501);
        assert_eq!(snapshot.logs.len(), 500);
    }

    #[test]
    fn opening_a_v1_metrics_file_resets_it_to_the_v2_schema() {
        let directory = TestDirectory::new();
        let path = directory.0.join("metrics.sqlite3");
        let legacy = rusqlite::Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                "
                CREATE TABLE button_metrics (
                    model_id TEXT NOT NULL,
                    button_id TEXT NOT NULL,
                    total_presses INTEGER NOT NULL,
                    last_pressed_at_ms INTEGER NOT NULL,
                    PRIMARY KEY (model_id, button_id)
                );
                CREATE TABLE button_metric_days (
                    model_id TEXT NOT NULL,
                    button_id TEXT NOT NULL,
                    day TEXT NOT NULL,
                    presses INTEGER NOT NULL,
                    PRIMARY KEY (model_id, button_id, day)
                );
                CREATE TABLE activity_logs (
                    id INTEGER PRIMARY KEY,
                    occurred_at_ms INTEGER NOT NULL,
                    kind TEXT NOT NULL,
                    message TEXT NOT NULL
                );
                INSERT INTO button_metrics VALUES ('phone', 'ONE', 9, 10);
                ",
            )
            .unwrap();
        drop(legacy);

        let store = MetricsStore::open(&path).unwrap();

        assert_eq!(store.backup().unwrap(), Default::default());
        let connection = rusqlite::Connection::open(path).unwrap();
        let columns = connection
            .prepare("PRAGMA table_info(button_metrics)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(columns.iter().any(|column| column == "device_profile_id"));
        assert!(!columns.iter().any(|column| column == "model_id"));
    }

    #[test]
    fn backup_validation_rejects_broken_metric_references() {
        let directory = TestDirectory::new();
        let store = MetricsStore::open(&directory.0.join("metrics.sqlite3")).unwrap();
        let attribution = MetricAttribution {
            device_id: DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap(),
            device_name: "Desk".into(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        store
            .record_button_press(&attribution, "ONE", 1_720_086_400_000)
            .unwrap();
        let mut excessive_day = store.backup().unwrap();
        excessive_day.button_metric_days[0].presses = 2;
        assert!(excessive_day.validate().is_err());

        let mut missing_day = store.backup().unwrap();
        missing_day.button_metric_days.clear();
        assert!(missing_day.validate().is_err());

        let mut undersummed_day = store.backup().unwrap();
        undersummed_day.button_metrics[0].total_presses = 2;
        assert!(undersummed_day.validate().is_err());

        let mut missing_aggregate = store.backup().unwrap();
        missing_aggregate.activity_logs[0].button_id = Some("MISSING".into());
        assert!(missing_aggregate.validate().is_err());
    }

    #[test]
    fn metrics_backup_uses_persistent_snake_case_fields() {
        let directory = TestDirectory::new();
        let store = MetricsStore::open(&directory.0.join("metrics.sqlite3")).unwrap();
        let attribution = MetricAttribution {
            device_id: DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap(),
            device_name: "Desk".into(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        store
            .record_button_press(&attribution, "ONE", 1_720_086_400_000)
            .unwrap();

        let yaml = serde_yaml_ng::to_string(&store.backup().unwrap()).unwrap();

        assert!(yaml.contains("button_metrics:"));
        assert!(yaml.contains("button_metric_days:"));
        assert!(yaml.contains("activity_logs:"));
        assert!(yaml.contains("occurred_at_ms:"));
        assert!(yaml.contains("device_profile_id:"));
    }

    #[test]
    fn seven_day_heatmap_excludes_future_days() {
        let directory = TestDirectory::new();
        let store = MetricsStore::open(&directory.0.join("metrics.sqlite3")).unwrap();
        let attribution = MetricAttribution {
            device_id: DeviceId::new("luatos-esp32s3-aio", "AAAAAAAAAAAA").unwrap(),
            device_name: "Desk".into(),
            device_profile_id: "phone".into(),
            hardware_profile_id: "esp-primary".into(),
        };
        let today = 1_720_086_400_000;
        store
            .record_button_press(&attribution, "ONE", today)
            .unwrap();
        store
            .record_button_press(&attribution, "ONE", today + 8 * 86_400_000)
            .unwrap();

        let snapshot = store.home_snapshot("phone", None, today).unwrap();

        assert_eq!(snapshot.heatmap.len(), 1);
        assert_eq!(snapshot.heatmap[0].presses, 1);
    }
}
