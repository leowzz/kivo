use std::time::{Duration, Instant};

use super::{
    CodexInputNeed, CodexTaskReader, CodexTerminalEvent, DisplayItem, DisplayPriority,
    DisplayProvider, DisplayState, MergedCodexTask, ProviderUpdate,
};

const SOURCE_ID: &str = "codex";
const TERMINAL_TTL: Duration = Duration::from_secs(8);
const ERROR_TTL: Duration = Duration::from_secs(15);

pub struct CodexDisplayProvider<R: CodexTaskReader> {
    source: R,
}

impl<R: CodexTaskReader> CodexDisplayProvider<R> {
    pub fn new(source: R) -> Self {
        Self { source }
    }
}

impl<R: CodexTaskReader> DisplayProvider for CodexDisplayProvider<R> {
    fn source_id(&self) -> &'static str {
        SOURCE_ID
    }

    fn poll(&mut self, now: Instant) -> Result<ProviderUpdate, &'static str> {
        let snapshot = self.source.poll_tasks(now)?;
        let running = snapshot.tasks.iter().filter(|task| task.running).count() as u32;
        let needs_input = snapshot
            .tasks
            .iter()
            .filter(|task| task.input_need.is_some())
            .count() as u32;
        let summary_state = if needs_input > 0 {
            DisplayState::NeedsInput
        } else if running > 0 {
            DisplayState::Running
        } else {
            DisplayState::Idle
        };
        let mut items = vec![
            DisplayItem::new(
                "codex.summary",
                SOURCE_ID,
                DisplayPriority::Ambient,
                summary_state,
                "Codex",
            )?
            .with_metric("running", running)
            .with_metric("needs_input", needs_input)
            .with_updated_at(now),
        ];
        items.extend(
            snapshot
                .tasks
                .iter()
                .map(task_item)
                .collect::<Result<Vec<_>, _>>()?,
        );

        Ok(ProviderUpdate {
            source: SOURCE_ID,
            health: snapshot.health,
            items,
        })
    }
}

fn task_item(task: &MergedCodexTask) -> Result<DisplayItem, &'static str> {
    let (state, detail, priority, ttl) = task_semantics(task);
    let title = task
        .cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| task.name.clone())
        .unwrap_or_else(|| task.thread_id.clone());
    let mut item = DisplayItem::new(
        format!("codex.task.{}", task.thread_id),
        SOURCE_ID,
        priority,
        state,
        title,
    )?
    .with_updated_at(task.updated_at);
    if let Some(detail) = detail {
        item = item.with_detail(detail);
    }
    if let Some(ttl) = ttl {
        item = item.with_expiry(task.updated_at + ttl);
    }
    Ok(item)
}

fn task_semantics(
    task: &MergedCodexTask,
) -> (
    DisplayState,
    Option<&'static str>,
    DisplayPriority,
    Option<Duration>,
) {
    if let Some(input_need) = task.input_need {
        let detail = match input_need {
            CodexInputNeed::UserInput => "user input requested",
            CodexInputNeed::Approval => "approval needed",
        };
        return (
            DisplayState::NeedsInput,
            Some(detail),
            DisplayPriority::Critical,
            None,
        );
    }
    if task.system_error {
        return (
            DisplayState::Error,
            None,
            DisplayPriority::Critical,
            Some(ERROR_TTL),
        );
    }
    match task.terminal_event {
        Some(CodexTerminalEvent::ResponseReady) => (
            DisplayState::Success,
            None,
            DisplayPriority::Attention,
            Some(TERMINAL_TTL),
        ),
        Some(CodexTerminalEvent::Interrupted) => (
            DisplayState::Warning,
            None,
            DisplayPriority::Attention,
            Some(TERMINAL_TTL),
        ),
        None if task.running => (DisplayState::Running, None, DisplayPriority::Normal, None),
        None => (DisplayState::Idle, None, DisplayPriority::Ambient, None),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::display::{
        CodexInputNeed, CodexSourceSnapshot, CodexTerminalEvent, DisplayProvider, DisplayState,
        MergedCodexTask, SourceHealth,
    };

    struct FakeCodexTaskReader {
        snapshot: Option<CodexSourceSnapshot>,
    }

    impl FakeCodexTaskReader {
        fn once(snapshot: CodexSourceSnapshot) -> Self {
            Self {
                snapshot: Some(snapshot),
            }
        }
    }

    impl CodexTaskReader for FakeCodexTaskReader {
        fn poll_tasks(&mut self, _now: Instant) -> Result<CodexSourceSnapshot, &'static str> {
            self.snapshot.take().ok_or("fake_exhausted")
        }
    }

    fn task(
        now: Instant,
        thread_id: &str,
        running: bool,
        input_need: Option<CodexInputNeed>,
    ) -> MergedCodexTask {
        MergedCodexTask {
            thread_id: thread_id.into(),
            name: None,
            cwd: PathBuf::from("/work/kivo"),
            updated_at: now,
            running,
            input_need,
            system_error: false,
            terminal_event: None,
            terminal_sequence: 0,
        }
    }

    fn metric(items: &[crate::display::DisplayItem], name: &str) -> u32 {
        *items
            .iter()
            .find(|item| item.id == "codex.summary")
            .unwrap()
            .metrics
            .get(name)
            .unwrap()
    }

    #[test]
    fn provider_maps_normalized_tasks_to_semantic_items() {
        let now = Instant::now();
        let source = FakeCodexTaskReader::once(CodexSourceSnapshot {
            health: SourceHealth::Healthy,
            tasks: vec![
                task(now, "thread-a", true, None),
                task(now, "thread-b", true, Some(CodexInputNeed::Approval)),
            ],
        });
        let mut provider = CodexDisplayProvider::new(source);

        let update = provider.poll(now).unwrap();
        assert_eq!(update.source, "codex");
        assert_eq!(metric(&update.items, "running"), 2);
        assert_eq!(metric(&update.items, "needs_input"), 1);
        assert_eq!(
            update
                .items
                .iter()
                .find(|item| item.id == "codex.task.thread-b")
                .unwrap()
                .detail,
            Some("approval needed".into())
        );
    }

    #[test]
    fn terminal_expiry_is_anchored_to_source_event_time() {
        let now = Instant::now();
        let mut ready = task(now, "thread-a", false, None);
        ready.terminal_event = Some(CodexTerminalEvent::ResponseReady);
        ready.terminal_sequence = 1;
        let source = FakeCodexTaskReader::once(CodexSourceSnapshot {
            health: SourceHealth::Healthy,
            tasks: vec![ready],
        });
        let mut provider = CodexDisplayProvider::new(source);

        let update = provider.poll(now + Duration::from_secs(4)).unwrap();
        let item = update
            .items
            .iter()
            .find(|item| item.id == "codex.task.thread-a")
            .unwrap();
        assert_eq!(item.state, DisplayState::Success);
        assert_eq!(item.expires_at, Some(now + Duration::from_secs(8)));
    }

    #[test]
    fn needs_input_outranks_error_and_running() {
        let now = Instant::now();
        let mut conflicted = task(now, "thread-a", true, Some(CodexInputNeed::UserInput));
        conflicted.system_error = true;
        let source = FakeCodexTaskReader::once(CodexSourceSnapshot {
            health: SourceHealth::Healthy,
            tasks: vec![conflicted],
        });
        let mut provider = CodexDisplayProvider::new(source);

        let update = provider.poll(now).unwrap();
        let item = update
            .items
            .iter()
            .find(|item| item.id == "codex.task.thread-a")
            .unwrap();
        assert_eq!(item.state, DisplayState::NeedsInput);
        assert_eq!(item.detail.as_deref(), Some("user input requested"));
    }

    #[test]
    fn provider_uses_raw_unicode_cwd_basename_before_task_name() {
        let now = Instant::now();
        let mut source_task = task(now, "a3f2-rest", false, None);
        source_task.name = Some("retained task name".into());
        source_task.cwd = PathBuf::from("/work/中文项目");
        source_task.system_error = true;
        let source = FakeCodexTaskReader::once(CodexSourceSnapshot {
            health: SourceHealth::Healthy,
            tasks: vec![source_task],
        });
        let mut provider = CodexDisplayProvider::new(source);

        let update = provider.poll(now).unwrap();

        assert_eq!(
            update
                .items
                .iter()
                .find(|item| item.id == "codex.task.a3f2-rest")
                .unwrap()
                .title,
            "中文项目"
        );
    }
}
