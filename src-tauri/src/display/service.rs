use std::{
    collections::BTreeMap,
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::runtime_log::{self, RuntimeLogEntry, RuntimeLogLevel};

use super::{
    DisplayHub, DisplayProvider, DisplaySnapshot, ProviderRegistry, ProviderUpdate, SourceHealth,
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProviderPollStatus {
    Available(SourceHealth),
    Unavailable(&'static str),
}

pub struct UnavailableDisplayProvider {
    source: &'static str,
    error_code: &'static str,
}

impl UnavailableDisplayProvider {
    pub fn new(source: &'static str, error_code: &'static str) -> Self {
        Self { source, error_code }
    }
}

impl DisplayProvider for UnavailableDisplayProvider {
    fn source_id(&self) -> &'static str {
        self.source
    }

    fn poll(&mut self, _now: Instant) -> Result<ProviderUpdate, &'static str> {
        Err(self.error_code)
    }
}

pub struct DisplayService;

impl DisplayService {
    pub fn spawn(
        mut providers: ProviderRegistry,
        stop: Arc<AtomicBool>,
        snapshots: mpsc::Sender<Arc<DisplaySnapshot>>,
    ) -> io::Result<JoinHandle<()>> {
        thread::Builder::new()
            .name("display-service".into())
            .spawn(move || {
                let mut hub = DisplayHub::default();
                let mut last_snapshot = None;
                let mut logged_statuses = BTreeMap::new();
                while !stop.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    for provider in providers.providers_mut() {
                        let source = provider.source_id();
                        let (status, item_count) = match provider.poll(now) {
                            Ok(update)
                                if update.source == source
                                    && update
                                        .items
                                        .iter()
                                        .all(|item| item.source == update.source) =>
                            {
                                let health = update.health;
                                let item_count = update.items.len();
                                hub.replace_source(now, source, health, update.items);
                                (ProviderPollStatus::Available(health), item_count)
                            }
                            Ok(update) => {
                                hub.mark_unavailable(now, source);
                                (
                                    ProviderPollStatus::Unavailable(
                                        "display_provider_source_mismatch",
                                    ),
                                    update.items.len(),
                                )
                            }
                            Err(error_code) => {
                                hub.mark_unavailable(now, source);
                                (ProviderPollStatus::Unavailable(error_code), 0)
                            }
                        };
                        if logged_statuses.get(source) != Some(&status) {
                            log_provider_status(source, status, item_count);
                            logged_statuses.insert(source, status);
                        }
                    }

                    let snapshot = Arc::new(hub.snapshot(now));
                    if last_snapshot.as_deref() != Some(snapshot.as_ref()) {
                        if snapshots.send(Arc::clone(&snapshot)).is_err() {
                            break;
                        }
                        last_snapshot = Some(snapshot);
                    }
                    thread::sleep(POLL_INTERVAL);
                }
            })
    }
}

fn log_provider_status(source: &'static str, status: ProviderPollStatus, item_count: usize) {
    let (level, health, error_code) = match status {
        ProviderPollStatus::Available(health) => (RuntimeLogLevel::Info, health_code(health), None),
        ProviderPollStatus::Unavailable(error_code) => {
            (RuntimeLogLevel::Warning, "unavailable", Some(error_code))
        }
    };
    runtime_log::emit(RuntimeLogEntry::new(
        now_ms(),
        level,
        "display_provider_status",
        serde_json::json!({
            "providerId": source,
            "health": health,
            "errorCode": error_code,
            "itemCount": item_count,
        }),
    ));
}

fn health_code(health: SourceHealth) -> &'static str {
    match health {
        SourceHealth::Healthy => "healthy",
        SourceHealth::Degraded => "degraded",
        SourceHealth::Stale => "stale",
        SourceHealth::Offline => "offline",
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };

    use super::{DisplayService, POLL_INTERVAL, UnavailableDisplayProvider};
    use crate::display::{
        DisplayItem, DisplayPriority, DisplayProvider, DisplayState, ProviderRegistry,
        ProviderUpdate, SourceHealth,
    };

    #[derive(Clone)]
    struct FakeUpdate {
        health: SourceHealth,
        title: &'static str,
        detail: Option<&'static str>,
        include_task: bool,
    }

    impl FakeUpdate {
        fn healthy() -> Self {
            Self {
                health: SourceHealth::Healthy,
                title: "Codex",
                detail: None,
                include_task: false,
            }
        }
    }

    struct FakeProvider {
        updates: VecDeque<FakeUpdate>,
        last: FakeUpdate,
    }

    impl FakeProvider {
        fn from_updates(updates: Vec<FakeUpdate>) -> Self {
            Self {
                updates: updates.into(),
                last: FakeUpdate::healthy(),
            }
        }
    }

    impl DisplayProvider for FakeProvider {
        fn source_id(&self) -> &'static str {
            "codex"
        }

        fn poll(&mut self, now: Instant) -> Result<ProviderUpdate, &'static str> {
            if let Some(update) = self.updates.pop_front() {
                self.last = update;
            }
            let mut summary = DisplayItem::new(
                "codex.summary",
                self.source_id(),
                DisplayPriority::Ambient,
                DisplayState::Running,
                self.last.title,
            )
            .unwrap()
            .with_updated_at(now);
            if let Some(detail) = self.last.detail {
                summary = summary.with_detail(detail);
            }
            let mut items = vec![summary];
            if self.last.include_task {
                items.push(
                    DisplayItem::new(
                        "codex.task.review",
                        self.source_id(),
                        DisplayPriority::Attention,
                        DisplayState::NeedsInput,
                        "review",
                    )
                    .unwrap()
                    .with_updated_at(now),
                );
            }
            Ok(ProviderUpdate {
                source: self.source_id(),
                health: self.last.health,
                items,
            })
        }
    }

    struct DropProbeProvider {
        dropped: Arc<AtomicBool>,
    }

    impl Drop for DropProbeProvider {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    impl DisplayProvider for DropProbeProvider {
        fn source_id(&self) -> &'static str {
            "codex"
        }

        fn poll(&mut self, _now: Instant) -> Result<ProviderUpdate, &'static str> {
            Err("test_unavailable")
        }
    }

    struct MismatchedSourceProvider {
        update_source: &'static str,
        item_source: &'static str,
    }

    impl DisplayProvider for MismatchedSourceProvider {
        fn source_id(&self) -> &'static str {
            "codex"
        }

        fn poll(&mut self, now: Instant) -> Result<ProviderUpdate, &'static str> {
            Ok(ProviderUpdate {
                source: self.update_source,
                health: SourceHealth::Healthy,
                items: vec![
                    DisplayItem::new(
                        "codex.summary",
                        self.item_source,
                        DisplayPriority::Ambient,
                        DisplayState::Running,
                        "Codex",
                    )
                    .unwrap()
                    .with_updated_at(now),
                ],
            })
        }
    }

    fn first_snapshot(provider: Box<dyn DisplayProvider>) -> Arc<crate::display::DisplaySnapshot> {
        let mut providers = ProviderRegistry::default();
        providers.register(provider).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, snapshots) = mpsc::channel();
        let service = DisplayService::spawn(providers, Arc::clone(&stop), sender).unwrap();
        let snapshot = snapshots.recv_timeout(Duration::from_secs(1)).unwrap();
        stop.store(true, Ordering::Relaxed);
        service.join().unwrap();
        snapshot
    }

    #[test]
    fn service_emits_only_semantically_changed_snapshots() {
        let initial = FakeUpdate::healthy();
        let degraded = FakeUpdate {
            health: SourceHealth::Degraded,
            ..initial.clone()
        };
        let renamed = FakeUpdate {
            title: "Kivo",
            ..degraded.clone()
        };
        let detailed = FakeUpdate {
            detail: Some("status changed"),
            ..renamed.clone()
        };
        let with_task = FakeUpdate {
            include_task: true,
            ..detailed.clone()
        };
        let mut providers = ProviderRegistry::default();
        providers
            .register(Box::new(FakeProvider::from_updates(vec![
                initial.clone(),
                initial,
                degraded,
                renamed,
                detailed,
                with_task,
            ])))
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, snapshots) = mpsc::channel();
        let service = DisplayService::spawn(providers, Arc::clone(&stop), sender).unwrap();

        let emitted = (0..5)
            .map(|_| snapshots.recv_timeout(Duration::from_secs(1)).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(emitted[0].health["codex"], SourceHealth::Healthy);
        assert_eq!(emitted[1].health["codex"], SourceHealth::Degraded);
        assert_eq!(emitted[2].items[0].title, "Kivo");
        assert_eq!(
            emitted[3].items[0].detail.as_deref(),
            Some("status changed")
        );
        assert_eq!(emitted[4].items.len(), 2);
        assert_eq!(emitted[4].items[0].id, "codex.task.review");
        assert!(snapshots.recv_timeout(Duration::from_millis(150)).is_err());

        stop.store(true, Ordering::Relaxed);
        service.join().unwrap();
    }

    #[test]
    fn service_stops_and_drops_providers_when_stop_is_set() {
        let dropped = Arc::new(AtomicBool::new(false));
        let mut providers = ProviderRegistry::default();
        providers
            .register(Box::new(DropProbeProvider {
                dropped: Arc::clone(&dropped),
            }))
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, _snapshots) = mpsc::channel();
        let service = DisplayService::spawn(providers, Arc::clone(&stop), sender).unwrap();

        stop.store(true, Ordering::Relaxed);
        service.join().unwrap();

        assert!(dropped.load(Ordering::Relaxed));
    }

    #[test]
    fn service_rejects_items_claiming_a_different_source() {
        let snapshot = first_snapshot(Box::new(MismatchedSourceProvider {
            update_source: "codex",
            item_source: "other",
        }));

        assert!(snapshot.items.is_empty());
        assert_eq!(
            snapshot.health,
            BTreeMap::from([("codex".to_owned(), SourceHealth::Offline)])
        );
    }

    #[test]
    fn service_rejects_updates_claiming_a_different_provider() {
        let snapshot = first_snapshot(Box::new(MismatchedSourceProvider {
            update_source: "other",
            item_source: "other",
        }));

        assert!(snapshot.items.is_empty());
        assert_eq!(
            snapshot.health,
            BTreeMap::from([("codex".to_owned(), SourceHealth::Offline)])
        );
        assert!(!snapshot.health.contains_key("other"));
    }

    #[test]
    fn unavailable_provider_returns_the_stable_source_error() {
        let mut provider = UnavailableDisplayProvider::new("codex", "codex_source_init");

        assert!(matches!(
            provider.poll(Instant::now()),
            Err("codex_source_init")
        ));
        assert!(matches!(
            provider.poll(Instant::now()),
            Err("codex_source_init")
        ));
    }

    #[test]
    fn polling_interval_stays_within_the_service_latency_budget() {
        assert!(POLL_INTERVAL <= Duration::from_millis(100));
    }

    #[test]
    fn receiver_disconnected_before_first_snapshot_stops_and_drops_providers() {
        let dropped = Arc::new(AtomicBool::new(false));
        let mut providers = ProviderRegistry::default();
        providers
            .register(Box::new(DropProbeProvider {
                dropped: Arc::clone(&dropped),
            }))
            .unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, snapshots) = mpsc::channel();
        drop(snapshots);

        DisplayService::spawn(providers, stop, sender)
            .unwrap()
            .join()
            .unwrap();

        assert!(dropped.load(Ordering::Relaxed));
    }
}
