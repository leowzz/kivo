use std::{collections::BTreeMap, time::Instant};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum DisplayPriority {
    Ambient,
    Normal,
    Attention,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayState {
    Idle,
    Running,
    NeedsInput,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayItem {
    pub id: String,
    pub source: String,
    pub priority: DisplayPriority,
    pub state: DisplayState,
    pub title: String,
    pub detail: Option<String>,
    pub metrics: BTreeMap<String, u32>,
    pub progress: Option<u8>,
    pub expires_at: Option<Instant>,
    pub updated_at: Instant,
}

impl DisplayItem {
    pub fn new(
        id: impl Into<String>,
        source: impl Into<String>,
        priority: DisplayPriority,
        state: DisplayState,
        title: impl Into<String>,
    ) -> Result<Self, &'static str> {
        let id = id.into();
        let source = source.into();
        if id.is_empty() || source.is_empty() {
            return Err("display_identity_empty");
        }
        Ok(Self {
            id,
            source,
            priority,
            state,
            title: title.into(),
            detail: None,
            metrics: BTreeMap::new(),
            progress: None,
            expires_at: None,
            updated_at: Instant::now(),
        })
    }

    pub fn with_progress(mut self, progress: u8) -> Result<Self, &'static str> {
        if progress > 100 {
            return Err("display_progress_out_of_range");
        }
        self.progress = Some(progress);
        Ok(self)
    }

    pub fn with_updated_at(mut self, updated_at: Instant) -> Self {
        self.updated_at = updated_at;
        self
    }

    pub fn with_expiry(mut self, expires_at: Instant) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_metric(mut self, name: impl Into<String>, value: u32) -> Self {
        self.metrics.insert(name.into(), value);
        self
    }

    pub fn key(&self) -> (&str, &str) {
        (&self.source, &self.id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceHealth {
    Healthy,
    Degraded,
    Stale,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplaySnapshot {
    pub items: Vec<DisplayItem>,
    pub health: BTreeMap<String, SourceHealth>,
}

impl DisplaySnapshot {
    pub fn health(&self, source: &str) -> SourceHealth {
        self.health
            .get(source)
            .copied()
            .unwrap_or(SourceHealth::Offline)
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayItem, DisplayPriority, DisplayState};

    #[test]
    fn rejects_progress_above_one_hundred() {
        let item = DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap()
        .with_progress(101);
        assert_eq!(item.unwrap_err(), "display_progress_out_of_range");
    }

    #[test]
    fn item_identity_is_source_plus_id() {
        let item = DisplayItem::new(
            "codex.summary",
            "codex",
            DisplayPriority::Ambient,
            DisplayState::Running,
            "Codex",
        )
        .unwrap();
        assert_eq!(item.key(), ("codex", "codex.summary"));
    }
}
