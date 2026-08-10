use std::{
    cmp::Ordering,
    collections::BTreeMap,
    time::{Duration, Instant},
};

use super::{DisplayItem, DisplaySnapshot, DisplayState, SourceHealth};

const STALE_AFTER: Duration = Duration::from_secs(5);
const OFFLINE_AFTER: Duration = Duration::from_secs(15);
const CODEX_SUMMARY_ID: &str = "codex.summary";

#[derive(Default)]
pub struct DisplayHub {
    sources: BTreeMap<String, SourceState>,
}

struct SourceState {
    items: BTreeMap<String, DisplayItem>,
    health: SourceHealth,
    last_healthy_at: Option<Instant>,
    unavailable_since: Option<Instant>,
}

impl SourceState {
    fn offline() -> Self {
        Self {
            items: BTreeMap::new(),
            health: SourceHealth::Offline,
            last_healthy_at: None,
            unavailable_since: None,
        }
    }
}

impl DisplayHub {
    pub fn replace_source(
        &mut self,
        now: Instant,
        source: impl Into<String>,
        health: SourceHealth,
        items: Vec<DisplayItem>,
    ) {
        let state = self
            .sources
            .entry(source.into())
            .or_insert_with(SourceState::offline);
        let mut replacement = BTreeMap::new();
        for mut item in items {
            if let Some(previous) = state.items.get(&item.id)
                && item_matches_except_updated_at(previous, &item)
            {
                item.updated_at = previous.updated_at;
            }
            replacement.insert(item.id.clone(), item);
        }
        state.items = replacement;
        state.health = health;
        if matches!(health, SourceHealth::Healthy | SourceHealth::Degraded) {
            state.last_healthy_at = Some(now);
            state.unavailable_since = None;
        }
    }

    pub fn mark_unavailable(&mut self, now: Instant, source: impl Into<String>) {
        let state = self
            .sources
            .entry(source.into())
            .or_insert_with(SourceState::offline);
        if state.unavailable_since.is_none() {
            state.unavailable_since = Some(now);
        }
    }

    pub fn snapshot(&mut self, now: Instant) -> DisplaySnapshot {
        let mut health = BTreeMap::new();
        let mut items = Vec::new();

        for (source, state) in &mut self.sources {
            state
                .items
                .retain(|_, item| item.expires_at.is_none_or(|expires_at| now < expires_at));
            let source_health = source_health_at(state, now);
            if source_health == SourceHealth::Offline {
                state.items.clear();
            } else if source_health == SourceHealth::Stale {
                state.items.retain(|id, item| {
                    id == CODEX_SUMMARY_ID || item.state == DisplayState::NeedsInput
                });
                if let Some(summary) = state.items.get_mut(CODEX_SUMMARY_ID) {
                    summary.state = DisplayState::Warning;
                }
            }
            health.insert(source.clone(), source_health);
            items.extend(state.items.values().cloned());
        }

        items.sort_by(display_item_order);
        DisplaySnapshot { items, health }
    }
}

fn item_matches_except_updated_at(left: &DisplayItem, right: &DisplayItem) -> bool {
    left.id == right.id
        && left.source == right.source
        && left.priority == right.priority
        && left.state == right.state
        && left.title == right.title
        && left.detail == right.detail
        && left.metrics == right.metrics
        && left.progress == right.progress
        && left.expires_at == right.expires_at
}

fn source_health_at(state: &SourceState, now: Instant) -> SourceHealth {
    let Some(unavailable_since) = state.unavailable_since else {
        return state.health;
    };
    let unavailable_for = now.saturating_duration_since(unavailable_since);
    if unavailable_for >= OFFLINE_AFTER {
        SourceHealth::Offline
    } else if unavailable_for >= STALE_AFTER {
        SourceHealth::Stale
    } else {
        state.health
    }
}

fn display_item_order(left: &DisplayItem, right: &DisplayItem) -> Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| display_state_rank(right.state).cmp(&display_state_rank(left.state)))
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| left.source.cmp(&right.source))
        .then_with(|| left.id.cmp(&right.id))
}

fn display_state_rank(state: DisplayState) -> u8 {
    match state {
        DisplayState::NeedsInput => 6,
        DisplayState::Error => 5,
        DisplayState::Success => 4,
        DisplayState::Warning => 3,
        DisplayState::Running => 2,
        DisplayState::Idle => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::DisplayHub;
    use crate::display::{DisplayItem, DisplayPriority, DisplayState, SourceHealth};

    fn summary(now: Instant) -> DisplayItem {
        DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap()
        .with_updated_at(now)
    }

    fn response_ready(now: Instant, expires_at: Instant) -> DisplayItem {
        DisplayItem::new(
            "codex.response_ready",
            "codex",
            DisplayPriority::Attention,
            DisplayState::Success,
            "Response ready",
        )
        .unwrap()
        .with_updated_at(now)
        .with_expiry(expires_at)
    }

    fn item(now: Instant, id: &str, state: DisplayState) -> DisplayItem {
        DisplayItem::new(id, "codex", DisplayPriority::Normal, state, id)
            .unwrap()
            .with_updated_at(now)
    }

    #[test]
    fn expires_transient_items_but_keeps_summary() {
        let now = Instant::now();
        let mut hub = DisplayHub::default();
        hub.replace_source(
            now,
            "codex",
            SourceHealth::Healthy,
            vec![
                summary(now),
                response_ready(now, now + Duration::from_secs(8)),
            ],
        );

        assert_eq!(hub.snapshot(now + Duration::from_secs(7)).items.len(), 2);
        assert_eq!(
            hub.snapshot(now + Duration::from_secs(8)).items,
            vec![summary(now)]
        );
    }

    #[test]
    fn source_only_goes_offline_when_both_channels_fail() {
        let now = Instant::now();
        let mut hub = DisplayHub::default();
        hub.replace_source(now, "codex", SourceHealth::Healthy, vec![summary(now)]);
        hub.replace_source(
            now + Duration::from_secs(1),
            "codex",
            SourceHealth::Degraded,
            vec![summary(now)],
        );
        assert_eq!(
            hub.snapshot(now + Duration::from_secs(14)).health("codex"),
            SourceHealth::Degraded
        );

        hub.mark_unavailable(now + Duration::from_secs(15), "codex");
        assert_eq!(
            hub.snapshot(now + Duration::from_secs(31)).health("codex"),
            SourceHealth::Offline
        );
    }

    #[test]
    fn stale_sources_keep_needs_input_but_remove_transient_terminal_items() {
        let now = Instant::now();
        let expires_at = now + Duration::from_secs(60);
        let mut hub = DisplayHub::default();
        hub.replace_source(
            now,
            "codex",
            SourceHealth::Healthy,
            vec![
                summary(now),
                item(now, "codex.review", DisplayState::NeedsInput),
                item(now, "codex.success", DisplayState::Success).with_expiry(expires_at),
                item(now, "codex.warning", DisplayState::Warning).with_expiry(expires_at),
                item(now, "codex.error", DisplayState::Error).with_expiry(expires_at),
            ],
        );
        hub.mark_unavailable(now, "codex");

        let snapshot = hub.snapshot(now + Duration::from_secs(5));

        assert_eq!(snapshot.health("codex"), SourceHealth::Stale);
        assert_eq!(
            snapshot
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["codex.review", "codex.summary"]
        );
        assert_eq!(snapshot.items[0].state, DisplayState::NeedsInput);
        assert_eq!(snapshot.items[1].state, DisplayState::Warning);
    }

    #[test]
    fn replacement_preserves_timestamp_when_item_semantics_are_unchanged() {
        let now = Instant::now();
        let mut hub = DisplayHub::default();
        hub.replace_source(now, "codex", SourceHealth::Healthy, vec![summary(now)]);
        hub.replace_source(
            now + Duration::from_secs(1),
            "codex",
            SourceHealth::Healthy,
            vec![summary(now + Duration::from_secs(1))],
        );

        assert_eq!(
            hub.snapshot(now + Duration::from_secs(1)).items[0].updated_at,
            now
        );
    }

    #[test]
    fn repeated_unavailable_marks_do_not_reset_the_stale_interval() {
        let now = Instant::now();
        let mut hub = DisplayHub::default();
        hub.replace_source(now, "codex", SourceHealth::Healthy, vec![summary(now)]);
        hub.mark_unavailable(now, "codex");
        hub.mark_unavailable(now + Duration::from_secs(4), "codex");

        assert_eq!(
            hub.snapshot(now + Duration::from_secs(5)).health("codex"),
            SourceHealth::Stale
        );
    }
}
