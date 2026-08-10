use std::{collections::BTreeSet, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodexRolloutEvent {
    Session { thread_id: String, cwd: PathBuf },
    TurnStarted { turn_id: String },
    TurnCompleted { turn_id: String },
    TurnInterrupted { turn_id: String },
    UserInputRequested { call_id: String },
    UserInputResolved { call_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexTerminalEvent {
    ResponseReady,
    Interrupted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexInputNeed {
    UserInput,
    Approval,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexTaskSnapshot {
    pub thread_id: String,
    pub cwd: PathBuf,
    pub running: bool,
    pub input_need: Option<CodexInputNeed>,
    pub terminal_sequence: u64,
    pub event: Option<CodexTerminalEvent>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodexRolloutParseError {
    incomplete: bool,
}

impl CodexRolloutParseError {
    pub fn code(self) -> &'static str {
        if self.incomplete {
            "incomplete_json"
        } else {
            "invalid_json"
        }
    }
}

/// Projects a rollout JSONL record into the lifecycle fields Kivo is allowed to retain.
pub fn parse_rollout_line(line: &str) -> Result<Option<CodexRolloutEvent>, CodexRolloutParseError> {
    let value: Value = serde_json::from_str(line).map_err(|error| CodexRolloutParseError {
        incomplete: error.is_eof(),
    })?;
    let Some(record_type) = value.get("type").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(payload) = value.get("payload") else {
        return Ok(None);
    };

    match record_type {
        "session_meta" => match (string_field(payload, "id"), string_field(payload, "cwd")) {
            (Some(thread_id), Some(cwd)) => Ok(Some(CodexRolloutEvent::Session {
                thread_id: thread_id.to_owned(),
                cwd: PathBuf::from(cwd),
            })),
            _ => Ok(None),
        },
        "event_msg" => parse_event_message(payload),
        "response_item" => parse_response_item(payload),
        _ => Ok(None),
    }
}

fn parse_event_message(
    payload: &Value,
) -> Result<Option<CodexRolloutEvent>, CodexRolloutParseError> {
    let Some(event_type) = string_field(payload, "type") else {
        return Ok(None);
    };
    let Some(turn_id) = string_field(payload, "turn_id") else {
        return Ok(None);
    };

    match event_type {
        "task_started" => Ok(Some(CodexRolloutEvent::TurnStarted {
            turn_id: turn_id.to_owned(),
        })),
        "task_complete" => Ok(Some(CodexRolloutEvent::TurnCompleted {
            turn_id: turn_id.to_owned(),
        })),
        "turn_aborted" if string_field(payload, "reason") == Some("interrupted") => {
            Ok(Some(CodexRolloutEvent::TurnInterrupted {
                turn_id: turn_id.to_owned(),
            }))
        }
        _ => Ok(None),
    }
}

fn parse_response_item(
    payload: &Value,
) -> Result<Option<CodexRolloutEvent>, CodexRolloutParseError> {
    match string_field(payload, "type") {
        Some("function_call") => match (
            string_field(payload, "name"),
            string_field(payload, "call_id"),
        ) {
            (Some("request_user_input"), Some(call_id)) => {
                Ok(Some(CodexRolloutEvent::UserInputRequested {
                    call_id: call_id.to_owned(),
                }))
            }
            _ => Ok(None),
        },
        Some("function_call_output") => Ok(string_field(payload, "call_id").map(|call_id| {
            CodexRolloutEvent::UserInputResolved {
                call_id: call_id.to_owned(),
            }
        })),
        _ => Ok(None),
    }
}

fn string_field<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

#[derive(Default)]
pub struct CodexRolloutIndex {
    session: Option<CodexSession>,
    open_turn_ids: BTreeSet<String>,
    user_input_call_ids: BTreeSet<String>,
    terminal_sequence: u64,
    event: Option<CodexTerminalEvent>,
}

#[derive(Clone)]
struct CodexSession {
    thread_id: String,
    cwd: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CodexRolloutCursorState {
    pub thread_id: String,
    pub cwd: PathBuf,
    pub open_turn_ids: BTreeSet<String>,
    pub open_call_ids: BTreeSet<String>,
}

impl CodexRolloutIndex {
    pub fn apply_line(&mut self, line: &str) -> Result<(), CodexRolloutParseError> {
        if let Some(event) = parse_rollout_line(line)? {
            self.apply_event(event, true);
        }
        Ok(())
    }

    pub fn apply_initial_scan<'a>(
        &mut self,
        lines: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), CodexRolloutParseError> {
        for line in lines {
            if let Some(event) = parse_rollout_line(line)? {
                self.apply_event(event, false);
            }
        }
        self.terminal_sequence = 0;
        self.event = None;
        Ok(())
    }

    pub fn current_tasks(&self) -> Vec<CodexTaskSnapshot> {
        let Some(session) = &self.session else {
            return Vec::new();
        };

        let input_need =
            (!self.user_input_call_ids.is_empty()).then_some(CodexInputNeed::UserInput);
        vec![CodexTaskSnapshot {
            thread_id: session.thread_id.clone(),
            cwd: session.cwd.clone(),
            running: !self.open_turn_ids.is_empty(),
            input_need,
            terminal_sequence: self.terminal_sequence,
            event: self.event,
        }]
    }

    pub fn cursor_state(&self) -> Option<CodexRolloutCursorState> {
        let session = self.session.as_ref()?;
        Some(CodexRolloutCursorState {
            thread_id: session.thread_id.clone(),
            cwd: session.cwd.clone(),
            open_turn_ids: self.open_turn_ids.clone(),
            open_call_ids: self.user_input_call_ids.clone(),
        })
    }

    pub fn restore_cursor_state(&mut self, state: CodexRolloutCursorState) {
        self.session = Some(CodexSession {
            thread_id: state.thread_id,
            cwd: state.cwd,
        });
        self.open_turn_ids = state.open_turn_ids;
        self.user_input_call_ids = state.open_call_ids;
        self.terminal_sequence = 0;
        self.event = None;
    }

    fn apply_event(&mut self, event: CodexRolloutEvent, emit_terminal_event: bool) {
        match event {
            CodexRolloutEvent::Session { thread_id, cwd } => {
                self.session = Some(CodexSession { thread_id, cwd });
                self.open_turn_ids.clear();
                self.user_input_call_ids.clear();
                self.terminal_sequence = 0;
                self.event = None;
            }
            CodexRolloutEvent::TurnStarted { turn_id } => {
                if self.session.is_some() {
                    self.open_turn_ids.insert(turn_id);
                    self.event = None;
                }
            }
            CodexRolloutEvent::TurnCompleted { turn_id } => {
                self.open_turn_ids.remove(&turn_id);
                self.record_terminal_event(CodexTerminalEvent::ResponseReady, emit_terminal_event);
            }
            CodexRolloutEvent::TurnInterrupted { turn_id } => {
                self.open_turn_ids.remove(&turn_id);
                self.record_terminal_event(CodexTerminalEvent::Interrupted, emit_terminal_event);
            }
            CodexRolloutEvent::UserInputRequested { call_id } => {
                if self.session.is_some() {
                    self.user_input_call_ids.insert(call_id);
                }
            }
            CodexRolloutEvent::UserInputResolved { call_id } => {
                self.user_input_call_ids.remove(&call_id);
            }
        }
    }

    fn record_terminal_event(&mut self, event: CodexTerminalEvent, emit_terminal_event: bool) {
        if self.session.is_some() && emit_terminal_event {
            self.terminal_sequence += 1;
            self.event = Some(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_running_input_ready_and_interrupted_lifecycle() {
        let mut index = CodexRolloutIndex::default();
        let fixture = include_str!("../../tests/fixtures/codex/rollout-lifecycle.jsonl");
        let mut states = Vec::new();
        for line in fixture.lines() {
            index.apply_line(line).unwrap();
            states.push(index.current_tasks());
        }
        assert!(states[1][0].running);
        assert_eq!(states[2][0].input_need, Some(CodexInputNeed::UserInput));
        assert_eq!(states[3][0].input_need, None);
        assert_eq!(states[4][0].event, Some(CodexTerminalEvent::ResponseReady));
        assert_eq!(states[4][0].terminal_sequence, 1);
        assert_eq!(states[6][0].event, Some(CodexTerminalEvent::Interrupted));
        assert_eq!(states[6][0].terminal_sequence, 2);
    }

    #[test]
    fn ignores_body_fields_and_unknown_events() {
        let line = r#"{"type":"response_item","payload":{"type":"message","content":"secret"}}"#;
        assert_eq!(parse_rollout_line(line).unwrap(), None);
        assert!(!format!("{:?}", parse_rollout_line(line)).contains("secret"));

        let unknown_abort = r#"{"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"turn-a","reason":"shutdown"}}"#;
        assert_eq!(parse_rollout_line(unknown_abort).unwrap(), None);
    }

    #[test]
    fn leaves_truncated_last_line_for_the_next_read() {
        assert_eq!(
            parse_rollout_line("{\"type\":").unwrap_err().code(),
            "incomplete_json"
        );
    }

    #[test]
    fn initial_scan_restores_open_state_without_terminal_event() {
        let mut index = CodexRolloutIndex::default();
        index
            .apply_initial_scan(
                r#"{"type":"session_meta","payload":{"id":"thread-a","cwd":"/work/kivo"}}
{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-a"}}
{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-a"}}
{"type":"event_msg","payload":{"type":"task_started","turn_id":"turn-b"}}
{"type":"response_item","payload":{"type":"function_call","name":"request_user_input","call_id":"call-b"}}"#
                    .lines(),
            )
            .unwrap();

        let task = &index.current_tasks()[0];
        assert!(task.running);
        assert_eq!(task.input_need, Some(CodexInputNeed::UserInput));
        assert_eq!(task.event, None);
        assert_eq!(task.terminal_sequence, 0);
    }
}
