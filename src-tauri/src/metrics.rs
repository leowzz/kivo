use rusqlite::{Connection, params};
use serde::Serialize;
use std::{path::Path, sync::Mutex};

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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonMetric {
    pub button_id: String,
    pub presses: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonDayMetric {
    pub button_id: String,
    pub day: String,
    pub presses: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityLog {
    pub timestamp_ms: u64,
    pub kind: String,
    pub message: String,
}

pub struct MetricsStore {
    connection: Mutex<Connection>,
}

impl MetricsStore {
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS button_metrics (
                model_id TEXT NOT NULL,
                button_id TEXT NOT NULL,
                total_presses INTEGER NOT NULL DEFAULT 0,
                last_pressed_at_ms INTEGER NOT NULL,
                PRIMARY KEY (model_id, button_id)
            );
            CREATE TABLE IF NOT EXISTS button_metric_days (
                model_id TEXT NOT NULL,
                button_id TEXT NOT NULL,
                day TEXT NOT NULL,
                presses INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (model_id, button_id, day)
            );
            CREATE TABLE IF NOT EXISTS activity_logs (
                id INTEGER PRIMARY KEY,
                occurred_at_ms INTEGER NOT NULL,
                kind TEXT NOT NULL,
                message TEXT NOT NULL
            );
            ",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn record_button_press(
        &self,
        model_id: &str,
        button_id: &str,
        timestamp_ms: u64,
    ) -> Result<(), rusqlite::Error> {
        let mut connection = self.connection.lock().unwrap_or_else(|error| error.into_inner());
        let transaction = connection.transaction()?;
        transaction.execute(
            "
            INSERT INTO button_metrics (model_id, button_id, total_presses, last_pressed_at_ms)
            VALUES (?1, ?2, 1, ?3)
            ON CONFLICT(model_id, button_id) DO UPDATE SET
                total_presses = total_presses + 1,
                last_pressed_at_ms = excluded.last_pressed_at_ms
            ",
            params![model_id, button_id, timestamp_ms],
        )?;
        transaction.execute(
            "
            INSERT INTO button_metric_days (model_id, button_id, day, presses)
            VALUES (?1, ?2, strftime('%Y-%m-%d', ?3 / 1000, 'unixepoch', 'localtime'), 1)
            ON CONFLICT(model_id, button_id, day) DO UPDATE SET presses = presses + 1
            ",
            params![model_id, button_id, timestamp_ms],
        )?;
        transaction.commit()
    }

    pub fn record_activity(
        &self,
        timestamp_ms: u64,
        kind: &str,
        message: &str,
    ) -> Result<(), rusqlite::Error> {
        let mut connection = self.connection.lock().unwrap_or_else(|error| error.into_inner());
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO activity_logs (occurred_at_ms, kind, message) VALUES (?1, ?2, ?3)",
            params![timestamp_ms, kind, message],
        )?;
        transaction.execute(
            "DELETE FROM activity_logs WHERE id NOT IN (SELECT id FROM activity_logs ORDER BY id DESC LIMIT 500)",
            [],
        )?;
        transaction.commit()
    }

    pub fn home_snapshot(
        &self,
        model_id: &str,
        timestamp_ms: u64,
    ) -> Result<HomeMetricsSnapshot, rusqlite::Error> {
        let connection = self.connection.lock().unwrap_or_else(|error| error.into_inner());
        let total_presses = connection.query_row(
            "SELECT COALESCE(SUM(total_presses), 0) FROM button_metrics WHERE model_id = ?1",
            [model_id],
            |row| row.get::<_, i64>(0),
        )? as u64;
        let today_presses = connection.query_row(
            "SELECT COALESCE(SUM(presses), 0) FROM button_metric_days WHERE model_id = ?1 AND day = strftime('%Y-%m-%d', ?2 / 1000, 'unixepoch', 'localtime')",
            params![model_id, timestamp_ms],
            |row| row.get::<_, i64>(0),
        )? as u64;
        let active_button_count = connection.query_row(
            "SELECT COUNT(*) FROM button_metric_days WHERE model_id = ?1 AND day = strftime('%Y-%m-%d', ?2 / 1000, 'unixepoch', 'localtime')",
            params![model_id, timestamp_ms],
            |row| row.get::<_, i64>(0),
        )? as u64;
        let top_button = connection
            .query_row(
                "SELECT button_id, total_presses FROM button_metrics WHERE model_id = ?1 ORDER BY total_presses DESC, button_id ASC LIMIT 1",
                [model_id],
                |row| Ok(ButtonMetric { button_id: row.get(0)?, presses: row.get::<_, i64>(1)? as u64 }),
            )
            .ok();
        let mut statement = connection.prepare(
            "SELECT button_id, day, presses FROM button_metric_days WHERE model_id = ?1 AND day >= date(?2 / 1000, 'unixepoch', 'localtime', '-6 days') ORDER BY day ASC, button_id ASC",
        )?;
        let heatmap = statement
            .query_map(params![model_id, timestamp_ms], |row| {
                Ok(ButtonDayMetric {
                    button_id: row.get(0)?,
                    day: row.get(1)?,
                    presses: row.get::<_, i64>(2)? as u64,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(HomeMetricsSnapshot {
            total_presses,
            today_presses,
            active_button_count,
            top_button,
            heatmap,
            logs: logs(&connection)?,
        })
    }
}

fn logs(connection: &Connection) -> Result<Vec<ActivityLog>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT occurred_at_ms, kind, message FROM activity_logs ORDER BY id DESC LIMIT 500",
    )?;
    statement
        .query_map([], |row| {
            Ok(ActivityLog {
                timestamp_ms: row.get::<_, i64>(0)? as u64,
                kind: row.get(1)?,
                message: row.get(2)?,
            })
        })?
        .collect()
}

#[cfg(test)]
mod tests {
    use super::MetricsStore;
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
    fn records_totals_days_and_retains_only_500_logs() {
        let directory = TestDirectory::new();
        let store = MetricsStore::open(&directory.0.join("metrics.sqlite3")).unwrap();
        let yesterday = 1_720_000_000_000;
        let today = yesterday + 86_400_000;
        store.record_button_press("phone", "ONE", yesterday).unwrap();
        store.record_button_press("phone", "ONE", today).unwrap();
        for number in 0..501 {
            store
                .record_activity(today + number, "device", &format!("event {number}"))
                .unwrap();
        }

        let snapshot = store.home_snapshot("phone", today).unwrap();
        assert_eq!(snapshot.total_presses, 2);
        assert_eq!(snapshot.today_presses, 1);
        assert_eq!(snapshot.active_button_count, 1);
        assert_eq!(
            snapshot.top_button.as_ref().map(|button| button.button_id.as_str()),
            Some("ONE")
        );
        assert_eq!(snapshot.logs.len(), 500);
        assert_eq!(snapshot.logs[0].message, "event 500");
    }
}
