#[cfg(test)]
use crate::profile::{TriggerActions, TriggerSettings};
use crate::{
    display::{DrawOperation, SceneMode, SceneUpdate},
    hardware::{BoardProfile, board_by_id},
    profile::{ActionTrigger, ButtonAction, HardwareProfile, InputSource, MediaCommand},
    workspace::AppError,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const HOST_PROTOCOL_VERSION: u16 = 7;
pub const DISPLAY_PROTOCOL_VERSION: u16 = 7;
pub const ACTION_RUN_PROTOCOL_VERSION: u16 = 6;
pub const OLED_PROTOCOL_VERSION: u16 = 4;
pub const ADVANCED_ACTION_PROTOCOL_VERSION: u16 = 5;
const MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 3;
const DISPLAY_WIDTH: u16 = 128;
const DISPLAY_HEIGHT: u16 = 32;
const DISPLAY_MAX_REGIONS: usize = 8;
const DISPLAY_MAX_OPERATIONS: usize = 24;
const DISPLAY_MAX_TEXT_BYTES: usize = 48;
const DISPLAY_ASCII_FONT_ID: u8 = 0;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PhysicalInput {
    Direct { gpio: u8 },
    Contact { source: u8, pin_a: u8, pin_b: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceMessage {
    Hello(HelloCapabilities),
    ConfigOk {
        revision: u32,
    },
    ConfigError {
        revision: u32,
        code: String,
    },
    State {
        event_id: u64,
        input: PhysicalInput,
        state: InputState,
    },
    Done {
        run_id: u64,
        step: u16,
    },
    LearnOk {
        revision: u32,
    },
    LearnDirect {
        gpio: u8,
        state: InputState,
    },
    LearnContact {
        pin_a: u8,
        pin_b: u8,
        state: InputState,
    },
    DisplayOk {
        revision: u32,
    },
    DisplayResync {
        current_revision: u32,
    },
    DisplayError {
        revision: u32,
        code: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloCapabilities {
    pub protocol: u16,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub firmware_build_id: String,
    pub pins: Vec<u8>,
}

pub fn validate_hello(
    candidate_board: &BoardProfile,
    hello: &HelloCapabilities,
) -> Result<(), AppError> {
    if !(MIN_SUPPORTED_PROTOCOL_VERSION..=HOST_PROTOCOL_VERSION).contains(&hello.protocol) {
        return Err(AppError::new("protocol_mismatch")
            .with_param("expected", HOST_PROTOCOL_VERSION.to_string())
            .with_param("actual", hello.protocol.to_string()));
    }
    if hello.controller_family_id != candidate_board.family_id {
        return Err(AppError::new("controller_family_mismatch")
            .with_param("expected", candidate_board.family_id)
            .with_param("actual", &hello.controller_family_id));
    }
    if hello.board_profile_id != candidate_board.id {
        return Err(AppError::new("board_profile_mismatch")
            .with_param("expected", candidate_board.id)
            .with_param("actual", &hello.board_profile_id));
    }
    if let Some(pin) = hello
        .pins
        .iter()
        .find(|pin| !candidate_board.safe_pins.contains(pin))
    {
        return Err(AppError::new("capability_mismatch").with_param("gpio", pin.to_string()));
    }
    Ok(())
}

pub fn parse_device(line: &str) -> Option<DeviceMessage> {
    if line.len() >= 255 {
        return None;
    }
    if is_hello_line(line) {
        return parse_hello(line);
    }
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["CONFIG_OK", revision] => Some(DeviceMessage::ConfigOk {
            revision: revision.parse().ok()?,
        }),
        ["CONFIG_ERROR", revision, code] => Some(DeviceMessage::ConfigError {
            revision: revision.parse().ok()?,
            code: (*code).to_owned(),
        }),
        ["STATE", event_id, "DIRECT", gpio, state] => {
            let event_id = event_id.parse().ok()?;
            (event_id > 0).then_some(DeviceMessage::State {
                event_id,
                input: PhysicalInput::Direct {
                    gpio: gpio.parse().ok()?,
                },
                state: parse_state(state)?,
            })
        }
        ["STATE", event_id, "CONTACT", source, pin_a, pin_b, state] => {
            let event_id = event_id.parse().ok()?;
            let (pin_a, pin_b) = normalized_pair(pin_a.parse().ok()?, pin_b.parse().ok()?);
            if event_id == 0 || pin_a == pin_b {
                return None;
            }
            Some(DeviceMessage::State {
                event_id,
                input: PhysicalInput::Contact {
                    source: source.parse().ok()?,
                    pin_a,
                    pin_b,
                },
                state: parse_state(state)?,
            })
        }
        ["DONE", run_id, step] => {
            let run_id = run_id.parse().ok()?;
            let step = step.parse().ok()?;
            (run_id > 0 && step > 0).then_some(DeviceMessage::Done { run_id, step })
        }
        ["LEARN_OK", revision] => Some(DeviceMessage::LearnOk {
            revision: revision.parse().ok()?,
        }),
        ["LEARN_DIRECT", gpio, state] => Some(DeviceMessage::LearnDirect {
            gpio: gpio.parse().ok()?,
            state: parse_state(state)?,
        }),
        ["LEARN_CONTACT", pin_a, pin_b, state] => {
            let (pin_a, pin_b) = normalized_pair(pin_a.parse().ok()?, pin_b.parse().ok()?);
            Some(DeviceMessage::LearnContact {
                pin_a,
                pin_b,
                state: parse_state(state)?,
            })
        }
        ["DISPLAY_OK", revision] => Some(DeviceMessage::DisplayOk {
            revision: revision.parse().ok()?,
        }),
        ["DISPLAY_RESYNC", current_revision] => Some(DeviceMessage::DisplayResync {
            current_revision: current_revision.parse().ok()?,
        }),
        ["DISPLAY_ERROR", revision, code] => Some(DeviceMessage::DisplayError {
            revision: revision.parse().ok()?,
            code: (*code).to_owned(),
        }),
        _ => None,
    }
}

pub(crate) fn display_commands(update: &SceneUpdate) -> Result<Vec<String>, String> {
    if update.new_revision == 0
        || match update.mode {
            SceneMode::Full => update.base_revision != 0,
            SceneMode::Delta => {
                update.base_revision == 0 || update.base_revision == update.new_revision
            }
        }
    {
        return Err("display_revision_invalid".into());
    }
    if update.regions.len() > DISPLAY_MAX_REGIONS {
        return Err("display_region_limit".into());
    }
    let operation_count = update
        .regions
        .iter()
        .try_fold(0usize, |count, region| {
            count.checked_add(region.operations.len())
        })
        .ok_or_else(|| "display_operation_limit".to_owned())?;
    if operation_count > DISPLAY_MAX_OPERATIONS {
        return Err("display_operation_limit".into());
    }

    let mode = match update.mode {
        SceneMode::Full => "full",
        SceneMode::Delta => "delta",
    };
    let mut lines = Vec::new();
    push_display_line(
        &mut lines,
        format!(
            "DISPLAY_BEGIN {} {} {mode}\n",
            update.new_revision, update.base_revision
        ),
    )?;
    for region in &update.regions {
        let right = region
            .bounds
            .x
            .checked_add(region.bounds.width)
            .filter(|right| region.bounds.width > 0 && *right <= DISPLAY_WIDTH);
        let bottom = region
            .bounds
            .y
            .checked_add(region.bounds.height)
            .filter(|bottom| region.bounds.height > 0 && *bottom <= DISPLAY_HEIGHT);
        let (Some(right), Some(bottom)) = (right, bottom) else {
            return Err("display_region_bounds".into());
        };
        push_display_line(
            &mut lines,
            format!(
                "DISPLAY_REGION {} {} {} {} {}\n",
                region.slot,
                region.bounds.x,
                region.bounds.y,
                region.bounds.width,
                region.bounds.height
            ),
        )?;
        for operation in &region.operations {
            match operation {
                DrawOperation::ClearRegion => {
                    push_display_line(&mut lines, format!("DISPLAY_CLEAR {}\n", region.slot))?;
                }
                DrawOperation::Text {
                    x,
                    baseline_y,
                    font_id,
                    text,
                } => {
                    if *x < region.bounds.x
                        || *x >= right
                        || *baseline_y < region.bounds.y
                        || *baseline_y >= bottom
                    {
                        return Err("display_text_bounds".into());
                    }
                    if *font_id != DISPLAY_ASCII_FONT_ID {
                        return Err("display_font_unsupported".into());
                    }
                    if text.len() > DISPLAY_MAX_TEXT_BYTES {
                        return Err("display_text_limit".into());
                    }
                    if !text.is_ascii() {
                        return Err("display_text_charset".into());
                    }
                    push_display_line(
                        &mut lines,
                        format!(
                            "DISPLAY_TEXT {} {x} {baseline_y} {font_id} {}\n",
                            region.slot,
                            STANDARD.encode(text.as_bytes())
                        ),
                    )?;
                }
            }
        }
    }
    push_display_line(
        &mut lines,
        format!("DISPLAY_COMMIT {}\n", update.new_revision),
    )?;
    Ok(lines)
}

fn push_display_line(lines: &mut Vec<String>, line: String) -> Result<(), String> {
    if line.len() >= 255 {
        return Err("display_line_limit".into());
    }
    lines.push(line);
    Ok(())
}

pub(crate) fn is_hello_line(line: &str) -> bool {
    line.trim_start_matches(char::is_whitespace)
        .starts_with("HELLO")
}

fn parse_hello(line: &str) -> Option<DeviceMessage> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty()
        || line.starts_with(' ')
        || line.ends_with(' ')
        || line
            .chars()
            .any(|character| character.is_whitespace() && character != ' ')
    {
        return None;
    }
    let parts = line.split(' ').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let [
        "HELLO",
        protocol,
        controller_family_id,
        board_profile_id,
        firmware_build_id,
        count,
        pins @ ..,
    ] = parts.as_slice()
    else {
        return None;
    };
    let protocol = protocol.parse::<u16>().ok()?;
    if !(MIN_SUPPORTED_PROTOCOL_VERSION..=HOST_PROTOCOL_VERSION).contains(&protocol) {
        return None;
    }
    let count = count.parse::<usize>().ok()?;
    let pins = pins
        .iter()
        .map(|pin| pin.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (count > 0
        && count == pins.len()
        && pins.iter().copied().collect::<BTreeSet<_>>().len() == count)
        .then(|| {
            DeviceMessage::Hello(HelloCapabilities {
                protocol,
                controller_family_id: (*controller_family_id).to_owned(),
                board_profile_id: (*board_profile_id).to_owned(),
                firmware_build_id: (*firmware_build_id).to_owned(),
                pins,
            })
        })
}

fn parse_state(value: &str) -> Option<InputState> {
    match value {
        "DOWN" => Some(InputState::Down),
        "UP" => Some(InputState::Up),
        _ => None,
    }
}

fn normalized_pair(left: u8, right: u8) -> (u8, u8) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

pub fn topology_commands(
    hardware: &HardwareProfile,
    revision: u32,
    reported_pins: &BTreeSet<u8>,
) -> Result<Vec<String>, AppError> {
    let board = board_by_id(&hardware.board_profile_id).ok_or_else(|| {
        AppError::new("unknown_board_profile")
            .with_param("board_profile", &hardware.board_profile_id)
    })?;
    if let Some(ssd1306) = &hardware.ssd1306 {
        if !board.supports_oled {
            return Err(AppError::new("oled_not_supported").with_param("board_profile", board.id));
        }
        if ssd1306.sda == ssd1306.scl {
            return Err(AppError::new("gpio_used_by_multiple_sources")
                .with_param("gpio", ssd1306.sda.to_string()));
        }
    }
    for pin in hardware_pins(hardware) {
        if !board.safe_pins.contains(&pin) || !reported_pins.contains(&pin) {
            return Err(AppError::new("capability_mismatch").with_param("gpio", pin.to_string()));
        }
    }
    let mut lines = vec![format!(
        "CONFIG_BEGIN {revision} {}\n",
        hardware.debounce_ms
    )];
    if let Some(ssd1306) = &hardware.ssd1306 {
        lines.push(format!(
            "CONFIG_OLED {revision} {} {}\n",
            ssd1306.sda, ssd1306.scl
        ));
    }
    let mut source_index = 0u8;
    for input in &hardware.inputs {
        match input {
            InputSource::Direct { keys, .. } if !keys.is_empty() => {
                let pins = keys.values().copied().collect::<BTreeSet<_>>();
                lines.push(format!(
                    "CONFIG_DIRECT {revision} {source_index} {} {}\n",
                    pins.len(),
                    join_pins(pins.iter().copied())
                ));
                source_index = source_index
                    .checked_add(1)
                    .ok_or_else(|| AppError::new("too_many_input_sources"))?;
            }
            InputSource::ContactMatrix { keys, .. } if !keys.is_empty() => {
                let (rows, columns) = matrix_partitions(keys.values().copied());
                lines.push(format!(
                    "CONFIG_MATRIX {revision} {source_index} {} {} {} {}\n",
                    rows.len(),
                    join_pins(rows.iter().copied()),
                    columns.len(),
                    join_pins(columns.iter().copied())
                ));
                source_index = source_index
                    .checked_add(1)
                    .ok_or_else(|| AppError::new("too_many_input_sources"))?;
            }
            InputSource::Direct { .. } | InputSource::ContactMatrix { .. } => {}
        }
    }
    lines.push(format!("CONFIG_COMMIT {revision}\n"));
    Ok(lines)
}

fn hardware_pins(hardware: &HardwareProfile) -> BTreeSet<u8> {
    let mut pins = hardware
        .inputs
        .iter()
        .flat_map(|input| match input {
            InputSource::Direct { keys, .. } => keys.values().copied().collect::<Vec<_>>(),
            InputSource::ContactMatrix { pins, .. } => pins.clone(),
        })
        .collect::<BTreeSet<_>>();
    if let Some(ssd1306) = &hardware.ssd1306 {
        pins.insert(ssd1306.sda);
        pins.insert(ssd1306.scl);
    }
    pins
}

fn matrix_partitions(pairs: impl IntoIterator<Item = [u8; 2]>) -> (Vec<u8>, Vec<u8>) {
    let mut neighbors: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
    for [left, right] in pairs {
        neighbors.entry(left).or_default().push(right);
        neighbors.entry(right).or_default().push(left);
    }
    let mut colors = BTreeMap::new();
    for &start in neighbors.keys() {
        if colors.contains_key(&start) {
            continue;
        }
        colors.insert(start, false);
        let mut queue = VecDeque::from([start]);
        while let Some(pin) = queue.pop_front() {
            let color = colors[&pin];
            for &neighbor in &neighbors[&pin] {
                if let std::collections::btree_map::Entry::Vacant(entry) = colors.entry(neighbor) {
                    entry.insert(!color);
                    queue.push_back(neighbor);
                }
            }
        }
    }
    colors.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut rows, mut columns), (pin, column)| {
            if column {
                columns.push(pin);
            } else {
                rows.push(pin);
            }
            (rows, columns)
        },
    )
}

fn join_pins(pins: impl Iterator<Item = u8>) -> String {
    pins.map(|pin| pin.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionStep {
    pub run_id: u64,
    pub button: String,
    pub trigger: ActionTrigger,
    pub step: u16,
    pub total: u16,
    pub action: ButtonAction,
}

impl ActionStep {
    pub fn command_legacy(
        &self,
        copy: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<String, String> {
        self.validate_coordinates()?;
        match &self.action {
            ButtonAction::Paste { text } => {
                copy(text)?;
                Ok(format_paste_command(self.run_id, self.step, self.total))
            }
            ButtonAction::Hotkey { keys } => {
                let chord = encode_hotkey(keys)?;
                let [keycode] = chord.keycodes.as_slice() else {
                    return Err("legacy hotkey protocol requires exactly one ordinary key".into());
                };
                Ok(format!(
                    "HOTKEY {} {} {} {} {keycode}\n",
                    self.run_id, self.step, self.total, chord.modifier_mask
                ))
            }
            ButtonAction::Delay { duration_ms } => Ok(format!(
                "DELAY {} {} {} {duration_ms}\n",
                self.run_id, self.step, self.total
            )),
            ButtonAction::Media { command } => Ok(format!(
                "MEDIA {} {} {} {}\n",
                self.run_id,
                self.step,
                self.total,
                media_usage(*command)
            )),
            ButtonAction::Open { .. } => Ok(format!(
                "HOST {} {} {}\n",
                self.run_id, self.step, self.total
            )),
        }
    }

    #[allow(dead_code)]
    pub fn command_v6(
        &self,
        copy: impl FnOnce(&str) -> Result<(), String>,
    ) -> Result<String, String> {
        self.validate_coordinates()?;
        match &self.action {
            ButtonAction::Paste { text } => {
                copy(text)?;
                Ok(format_paste_command(self.run_id, self.step, self.total))
            }
            ButtonAction::Hotkey { keys } => {
                let chord = encode_hotkey(keys)?;
                let keycodes = chord.keycodes.iter().map(u8::to_string).collect::<Vec<_>>();
                let mut command = format!(
                    "CHORD {} {} {} {} {}",
                    self.run_id,
                    self.step,
                    self.total,
                    chord.modifier_mask,
                    keycodes.len(),
                );
                if !keycodes.is_empty() {
                    command.push(' ');
                    command.push_str(&keycodes.join(" "));
                }
                command.push('\n');
                if command.len() >= 255 {
                    return Err("action command exceeds protocol line limit".into());
                }
                Ok(command)
            }
            ButtonAction::Delay { duration_ms } => Ok(format!(
                "DELAY {} {} {} {duration_ms}\n",
                self.run_id, self.step, self.total
            )),
            ButtonAction::Media { command } => Ok(format!(
                "MEDIA {} {} {} {}\n",
                self.run_id,
                self.step,
                self.total,
                media_usage(*command)
            )),
            ButtonAction::Open { .. } => Ok(format!(
                "HOST {} {} {}\n",
                self.run_id, self.step, self.total
            )),
        }
    }

    fn validate_coordinates(&self) -> Result<(), String> {
        if self.run_id == 0 || self.step == 0 || self.total == 0 || self.step > self.total {
            return Err("invalid action run coordinates".into());
        }
        Ok(())
    }
}

pub(crate) fn format_paste_command(run_id: u64, step: u16, total: u16) -> String {
    if cfg!(target_os = "macos") {
        format!("PASTE {run_id} {step} {total}\n")
    } else if cfg!(target_os = "windows") {
        format!("HOTKEY {run_id} {step} {total} 3 25\n")
    } else {
        format!("HOTKEY {run_id} {step} {total} 1 25\n")
    }
}

#[derive(Clone, Debug)]
pub struct ActionSequence {
    run_id: u64,
    button: String,
    trigger: ActionTrigger,
    actions: Vec<ButtonAction>,
    next: usize,
    awaiting: Option<u16>,
    failed: bool,
}

impl ActionSequence {
    pub fn new(
        run_id: u64,
        button: String,
        trigger: ActionTrigger,
        actions: Vec<ButtonAction>,
    ) -> Self {
        Self {
            run_id,
            button,
            trigger,
            actions,
            next: 0,
            awaiting: None,
            failed: false,
        }
    }

    pub fn next_step(&mut self) -> Option<ActionStep> {
        if self.failed || self.awaiting.is_some() || self.next >= self.actions.len() {
            return None;
        }
        let step = u16::try_from(self.next + 1).ok()?;
        let total = u16::try_from(self.actions.len()).ok()?;
        self.awaiting = Some(step);
        Some(ActionStep {
            run_id: self.run_id,
            button: self.button.clone(),
            trigger: self.trigger,
            step,
            total,
            action: self.actions[self.next].clone(),
        })
    }

    pub fn acknowledge(&mut self, run_id: u64, step: u16) -> Result<ActionStep, String> {
        if run_id != self.run_id || self.awaiting != Some(step) {
            self.failed = true;
            return Err("invalid_action_acknowledgement".into());
        }
        let completed = ActionStep {
            run_id: self.run_id,
            button: self.button.clone(),
            trigger: self.trigger,
            step,
            total: u16::try_from(self.actions.len()).map_err(|_| "invalid_action_count")?,
            action: self.actions[self.next].clone(),
        };
        self.awaiting = None;
        self.next += 1;
        Ok(completed)
    }

    pub fn abort(&mut self) {
        self.failed = true;
        self.awaiting = None;
    }

    pub fn run_id(&self) -> u64 {
        self.run_id
    }

    pub fn is_complete(&self) -> bool {
        !self.failed && self.next == self.actions.len() && self.awaiting.is_none()
    }

    pub fn is_waiting(&self) -> bool {
        self.awaiting.is_some() && !self.failed
    }

    pub fn is_awaiting_paste(&self) -> bool {
        self.is_waiting()
            && matches!(
                self.actions.get(self.next),
                Some(ButtonAction::Paste { .. })
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputState {
    Down,
    Up,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedChord {
    pub modifier_mask: u8,
    pub keycodes: Vec<u8>,
}

pub fn encode_hotkey(keys: &[String]) -> Result<EncodedChord, String> {
    if keys.is_empty() {
        return Err("empty hotkey".into());
    }

    let mut modifier_mask = 0;
    let mut keycodes = BTreeSet::new();
    for key in keys {
        let key = key.to_ascii_lowercase();
        let modifier = match key.as_str() {
            "primary" if cfg!(target_os = "macos") => Some(0x08),
            "primary" => Some(0x01),
            "ctrl" => Some(0x01),
            "shift" => Some(0x02),
            "alt" | "option" => Some(0x04),
            "cmd" => Some(0x08),
            "left_ctrl" => Some(0x01),
            "left_shift" => Some(0x02),
            "left_alt" => Some(0x04),
            "left_cmd" => Some(0x08),
            "right_ctrl" => Some(0x10),
            "right_shift" => Some(0x20),
            "right_alt" => Some(0x40),
            "right_cmd" => Some(0x80),
            _ => None,
        };
        if let Some(modifier) = modifier {
            if modifier_mask & modifier != 0 {
                return Err(format!("duplicate modifier {key}"));
            }
            modifier_mask |= modifier;
            continue;
        }
        let function_key = key
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .and_then(|number| match number {
                1..=12 => Some(0x3a + number - 1),
                13..=24 => Some(0x68 + number - 13),
                _ => None,
            });
        let numpad_digit = key
            .strip_prefix("numpad_")
            .and_then(|number| number.parse::<u8>().ok())
            .and_then(|number| match number {
                1..=9 => Some(0x59 + number - 1),
                0 => Some(0x62),
                _ => None,
            });
        let code = match key.as_bytes() {
            [letter @ b'a'..=b'z'] => letter - b'a' + 0x04,
            [digit @ b'1'..=b'9'] => digit - b'1' + 0x1e,
            b"0" => 0x27,
            b"enter" => 0x28,
            b"escape" => 0x29,
            b"backspace" => 0x2a,
            b"tab" => 0x2b,
            b"space" => 0x2c,
            b"minus" => 0x2d,
            b"equal" => 0x2e,
            b"left_bracket" => 0x2f,
            b"right_bracket" => 0x30,
            b"backslash" => 0x31,
            b"semicolon" => 0x33,
            b"quote" => 0x34,
            b"backtick" => 0x35,
            b"comma" => 0x36,
            b"period" => 0x37,
            b"slash" => 0x38,
            b"caps_lock" => 0x39,
            b"print_screen" => 0x46,
            b"scroll_lock" => 0x47,
            b"pause" => 0x48,
            b"insert" => 0x49,
            b"home" => 0x4a,
            b"pageup" | b"page_up" => 0x4b,
            b"delete" => 0x4c,
            b"end" => 0x4d,
            b"pagedown" | b"page_down" => 0x4e,
            b"right" => 0x4f,
            b"left" => 0x50,
            b"down" => 0x51,
            b"up" => 0x52,
            b"num_lock" => 0x53,
            b"numpad_divide" => 0x54,
            b"numpad_multiply" => 0x55,
            b"numpad_subtract" => 0x56,
            b"numpad_add" => 0x57,
            b"numpad_enter" => 0x58,
            b"numpad_decimal" => 0x63,
            b"application" => 0x65,
            b"numpad_equal" => 0x67,
            _ if function_key.is_some() => function_key.unwrap(),
            _ if numpad_digit.is_some() => numpad_digit.unwrap(),
            _ => return Err(format!("unknown key {key}")),
        };
        if !keycodes.insert(code) {
            return Err(format!("duplicate key {key}"));
        }
    }
    if keycodes.len() > 6 {
        return Err("too many ordinary keys".into());
    }
    Ok(EncodedChord {
        modifier_mask,
        keycodes: keycodes.into_iter().collect(),
    })
}

pub fn media_usage(command: MediaCommand) -> u16 {
    match command {
        MediaCommand::PlayPause => 0x00cd,
        MediaCommand::PreviousTrack => 0x00b6,
        MediaCommand::NextTrack => 0x00b5,
        MediaCommand::Stop => 0x00b7,
        MediaCommand::VolumeUp => 0x00e9,
        MediaCommand::VolumeDown => 0x00ea,
        MediaCommand::Mute => 0x00e2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        display::{DisplayRegion, DrawOperation, Rect, SceneMode, SceneUpdate},
        hardware::board_by_id,
        model::{ButtonDefinition, ButtonGroup, ModelLayout},
        profile::{DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION},
    };
    use std::collections::BTreeMap;

    fn device_profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: ModelLayout {
                id: "phone".into(),
                name: "电话".into(),
                groups: vec![ButtonGroup {
                    id: "keys".into(),
                    columns: 2,
                    buttons: vec![
                        ButtonDefinition {
                            id: "A".into(),
                            label: "甲".into(),
                        },
                        ButtonDefinition {
                            id: "B".into(),
                            label: "乙".into(),
                        },
                    ],
                }],
            },
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![HardwareProfile {
                id: "esp-primary".into(),
                name: "ESP primary".into(),
                board_profile_id: "luatos-esp32s3-aio".into(),
                debounce_ms: 30,
                ssd1306: None,
                inputs: vec![InputSource::ContactMatrix {
                    id: "matrix".into(),
                    pins: vec![1, 2, 12, 13],
                    keys: BTreeMap::from([("A".into(), [1, 12]), ("B".into(), [2, 13])]),
                }],
            }],
            actions: BTreeMap::from([(
                "A".into(),
                TriggerActions::press(vec![
                    ButtonAction::Paste {
                        text: "第一步".into(),
                    },
                    ButtonAction::Paste {
                        text: "第二步".into(),
                    },
                ]),
            )]),
        }
    }

    fn ssd1306_hardware_for(board_profile_id: &str, sda: u8, scl: u8) -> HardwareProfile {
        serde_yaml_ng::from_str(&format!(
            concat!(
                "id: rp-primary\n",
                "name: RP primary\n",
                "board_profile_id: {board_profile_id}\n",
                "debounce_ms: 30\n",
                "ssd1306:\n",
                "  sda: {sda}\n",
                "  scl: {scl}\n",
                "inputs:\n",
                "  - type: direct\n",
                "    id: direct\n",
                "    keys:\n",
                "      A: 6\n",
            ),
            board_profile_id = board_profile_id,
            sda = sda,
            scl = scl,
        ))
        .unwrap()
    }

    fn ssd1306_hardware() -> HardwareProfile {
        ssd1306_hardware_for("vccgnd-yd-rp2040", 4, 5)
    }

    fn display_update(
        new_revision: u32,
        base_revision: u32,
        mode: SceneMode,
        text: &str,
    ) -> SceneUpdate {
        SceneUpdate {
            new_revision,
            base_revision,
            mode,
            regions: vec![DisplayRegion::new(
                1,
                "row0_right",
                Rect::new(64, 0, 64, 16),
                vec![
                    DrawOperation::ClearRegion,
                    DrawOperation::Text {
                        x: 64,
                        baseline_y: 12,
                        font_id: 0,
                        text: text.into(),
                    },
                ],
            )],
        }
    }

    #[test]
    fn encodes_bounded_display_delta_with_base64_text() {
        let update = display_update(2, 1, SceneMode::Delta, "4 RUN");

        assert_eq!(
            display_commands(&update).unwrap(),
            vec![
                "DISPLAY_BEGIN 2 1 delta\n",
                "DISPLAY_REGION 1 64 0 64 16\n",
                "DISPLAY_CLEAR 1\n",
                "DISPLAY_TEXT 1 64 12 0 NCBSVU4=\n",
                "DISPLAY_COMMIT 2\n",
            ]
        );
    }

    #[test]
    fn parses_display_ack_resync_and_error() {
        assert_eq!(
            parse_device("DISPLAY_OK 9\n"),
            Some(DeviceMessage::DisplayOk { revision: 9 })
        );
        assert_eq!(
            parse_device("DISPLAY_RESYNC 7\n"),
            Some(DeviceMessage::DisplayResync {
                current_revision: 7
            })
        );
        assert_eq!(
            parse_device("DISPLAY_ERROR 9 invalid_text\n"),
            Some(DeviceMessage::DisplayError {
                revision: 9,
                code: "invalid_text".into(),
            })
        );
    }

    #[test]
    fn rejects_display_region_and_operation_count_overflow() {
        let mut too_many_regions = display_update(2, 1, SceneMode::Delta, "4 RUN");
        too_many_regions.regions = (0..9)
            .map(|slot| {
                DisplayRegion::new(
                    slot,
                    "test",
                    Rect::new(0, 0, 8, 8),
                    vec![DrawOperation::ClearRegion],
                )
            })
            .collect();
        assert_eq!(
            display_commands(&too_many_regions).unwrap_err(),
            "display_region_limit"
        );

        let mut too_many_operations = display_update(2, 1, SceneMode::Delta, "4 RUN");
        too_many_operations.regions[0].operations = vec![DrawOperation::ClearRegion; 25];
        assert_eq!(
            display_commands(&too_many_operations).unwrap_err(),
            "display_operation_limit"
        );
    }

    #[test]
    fn rejects_display_region_and_text_coordinates_outside_the_panel_or_slot() {
        let mut invalid_region = display_update(2, 1, SceneMode::Delta, "4 RUN");
        invalid_region.regions[0] = DisplayRegion::new(
            1,
            "row0_right",
            Rect::new(120, 0, 16, 16),
            vec![DrawOperation::ClearRegion],
        );
        assert_eq!(
            display_commands(&invalid_region).unwrap_err(),
            "display_region_bounds"
        );

        let mut invalid_text = display_update(2, 1, SceneMode::Delta, "4 RUN");
        invalid_text.regions[0].operations[1] = DrawOperation::Text {
            x: 63,
            baseline_y: 12,
            font_id: 0,
            text: "4 RUN".into(),
        };
        assert_eq!(
            display_commands(&invalid_text).unwrap_err(),
            "display_text_bounds"
        );
    }

    #[test]
    fn rejects_unsupported_or_oversized_display_text() {
        let oversized = display_update(2, 1, SceneMode::Delta, &"A".repeat(49));
        assert_eq!(
            display_commands(&oversized).unwrap_err(),
            "display_text_limit"
        );

        let non_ascii = display_update(2, 1, SceneMode::Delta, "运行");
        assert_eq!(
            display_commands(&non_ascii).unwrap_err(),
            "display_text_charset"
        );

        let mut unsupported_font = display_update(2, 1, SceneMode::Delta, "4 RUN");
        let DrawOperation::Text { font_id, .. } = &mut unsupported_font.regions[0].operations[1]
        else {
            unreachable!();
        };
        *font_id = 1;
        assert_eq!(
            display_commands(&unsupported_font).unwrap_err(),
            "display_font_unsupported"
        );
    }

    #[test]
    fn rejects_invalid_display_revisions_and_bounds_every_finished_line() {
        let invalid_full = display_update(1, 1, SceneMode::Full, "4 RUN");
        assert_eq!(
            display_commands(&invalid_full).unwrap_err(),
            "display_revision_invalid"
        );
        let invalid_delta = display_update(2, 0, SceneMode::Delta, "4 RUN");
        assert_eq!(
            display_commands(&invalid_delta).unwrap_err(),
            "display_revision_invalid"
        );

        let lines = display_commands(&display_update(2, 1, SceneMode::Delta, "4 RUN")).unwrap();
        assert!(lines.iter().all(|line| line.len() < 255));
    }

    #[test]
    fn parses_contact_state_and_done() {
        assert_eq!(
            parse_device("STATE 9 CONTACT 0 12 1 DOWN\n"),
            Some(DeviceMessage::State {
                event_id: 9,
                input: PhysicalInput::Contact {
                    source: 0,
                    pin_a: 1,
                    pin_b: 12,
                },
                state: InputState::Down,
            })
        );
        assert_eq!(
            parse_device("DONE 9 2\n"),
            Some(DeviceMessage::Done { run_id: 9, step: 2 })
        );
    }

    #[test]
    fn accepts_at_most_254_byte_protocol_lines() {
        let at_limit = format!("CONFIG_ERROR 1 {}\n", "x".repeat(238));
        assert_eq!(at_limit.len(), 254);
        assert!(matches!(
            parse_device(&at_limit),
            Some(DeviceMessage::ConfigError { revision: 1, .. })
        ));

        let over_limit = format!("CONFIG_ERROR 1 {}\n", "x".repeat(239));
        assert_eq!(over_limit.len(), 255);
        assert!(parse_device(&over_limit).is_none());
    }

    #[test]
    fn parses_protocol_v4_identity_and_build() {
        let message =
            parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();
        assert_eq!(
            message,
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 4,
                controller_family_id: "rp2040".into(),
                board_profile_id: "vccgnd-yd-rp2040".into(),
                firmware_build_id: "0.1.0+gabc1234".into(),
                pins: vec![0, 11, 22],
            })
        );
        assert!(
            parse_device("HELLO 4 esp32s3 luatos-esp32s3-aio 0.1.0+gabc1234 3 0 6 18",).is_some()
        );
        assert!(parse_device(
            "HELLO 4 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 23 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22",
        )
        .is_some());
    }

    #[test]
    fn parses_and_validates_protocol_v4_identity_and_build() {
        let message =
            parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();
        let DeviceMessage::Hello(hello) = message else {
            panic!("expected HELLO");
        };

        assert_eq!(hello.protocol, 4);
        assert!(validate_hello(board_by_id("vccgnd-yd-rp2040").unwrap(), &hello).is_ok());
    }

    #[test]
    fn parses_protocol_v3_hello_for_backward_compatibility() {
        let message =
            parse_device("HELLO 3 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();

        assert!(matches!(
            message,
            DeviceMessage::Hello(HelloCapabilities { protocol: 3, .. })
        ));
    }

    #[test]
    fn parses_protocol_v6_hello() {
        let message =
            parse_device("HELLO 6 rp2040 vccgnd-yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();

        assert!(matches!(
            message,
            DeviceMessage::Hello(HelloCapabilities { protocol: 6, .. })
        ));
    }

    #[test]
    fn parses_protocol_v5_and_v7_hello_compatibility_fixtures() {
        for protocol in [5, 7] {
            let message = parse_device(&format!(
                "HELLO {protocol} rp2040 vccgnd-yd-rp2040 build 3 0 11 22"
            ))
            .unwrap();
            assert!(matches!(
                message,
                DeviceMessage::Hello(HelloCapabilities {
                    protocol: actual,
                    ..
                }) if actual == protocol
            ));
        }
    }

    #[test]
    fn rejects_unsupported_or_malformed_hello_capabilities() {
        assert!(parse_device("HELLO 2 esp32s3 luatos-esp32s3-aio build 2 1 2").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040  3 2 0 1").is_none());
        assert!(parse_device("HELLO\t4\trp2040\tvccgnd-yd-rp2040\tbuild\t2\t0\t1").is_none());
        assert!(parse_device(" HELLO 4 rp2040 vccgnd-yd-rp2040 build 2 0 1").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 2 0 1 ").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 2 0 1\n").is_some());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 2 1").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 0").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 2 1 1").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 1 256").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 1 -1").is_none());
        assert!(parse_device("HELLO 4 rp2040 vccgnd-yd-rp2040 build 1 1 trailing").is_none());
        assert!(parse_device("STATE 9 DIRECT 6 DOWN trailing\n").is_none());
        assert!(parse_device("STATE 9 CONTACT 0 1 1 DOWN\n").is_none());
        assert!(parse_device("DONE 9 0\n").is_none());
        assert!(parse_device(&"x".repeat(256)).is_none());
    }

    #[test]
    fn validates_hello_against_the_classified_board() {
        let board = board_by_id("vccgnd-yd-rp2040").unwrap();
        let hello = HelloCapabilities {
            protocol: 4,
            controller_family_id: "rp2040".into(),
            board_profile_id: "vccgnd-yd-rp2040".into(),
            firmware_build_id: "test".into(),
            pins: vec![0, 22],
        };
        assert!(validate_hello(board, &hello).is_ok());

        let mut legacy_protocol = hello.clone();
        legacy_protocol.protocol = 3;
        assert!(validate_hello(board, &legacy_protocol).is_ok());

        let mut wrong_protocol = hello.clone();
        wrong_protocol.protocol = 2;
        assert_eq!(
            validate_hello(board, &wrong_protocol).unwrap_err().code,
            "protocol_mismatch"
        );

        let mut wrong_family = hello.clone();
        wrong_family.controller_family_id = "esp32s3".into();
        assert_eq!(
            validate_hello(board, &wrong_family).unwrap_err().code,
            "controller_family_mismatch"
        );

        let mut wrong_board = hello.clone();
        wrong_board.board_profile_id = "luatos-esp32s3-aio".into();
        assert_eq!(
            validate_hello(board, &wrong_board).unwrap_err().code,
            "board_profile_mismatch"
        );

        let mut unsafe_pin = hello;
        unsafe_pin.pins = vec![23];
        assert_eq!(
            validate_hello(board, &unsafe_pin).unwrap_err().code,
            "capability_mismatch"
        );
    }

    #[test]
    fn validates_legacy_rp2040_safe_pin_capability_subset() {
        let board = board_by_id("vccgnd-yd-rp2040").unwrap();
        let hello = HelloCapabilities {
            protocol: 3,
            controller_family_id: "rp2040".into(),
            board_profile_id: "vccgnd-yd-rp2040".into(),
            firmware_build_id: "legacy".into(),
            pins: (0..=22).collect(),
        };

        assert!(validate_hello(board, &hello).is_ok());
    }

    #[test]
    fn waits_for_done_before_returning_the_next_action() {
        let model = device_profile();
        let actions = model.actions["A"].press.clone();
        let first_action = actions[0].clone();
        let mut sequence = ActionSequence::new(9, "A".into(), ActionTrigger::Press, actions);

        assert_eq!(sequence.next_step().unwrap().step, 1);
        assert!(sequence.next_step().is_none());
        let completed = sequence.acknowledge(9, 1).unwrap();
        assert_eq!(completed.run_id, 9);
        assert_eq!(completed.button, "A");
        assert_eq!(completed.step, 1);
        assert_eq!(completed.total, 2);
        assert_eq!(completed.action, first_action);
        assert_eq!(sequence.next_step().unwrap().step, 2);
    }

    #[test]
    fn sequences_paste_delay_and_media_in_one_protocol_v6_run() {
        let actions = vec![
            ButtonAction::Paste {
                text: "你好\nKivo".into(),
            },
            ButtonAction::Delay { duration_ms: 500 },
            ButtonAction::Media {
                command: MediaCommand::PlayPause,
            },
        ];
        let mut sequence = ActionSequence::new(77, "A".into(), ActionTrigger::Press, actions);
        let mut commands = Vec::new();
        let mut clipboard = Vec::new();

        for expected_step in 1..=3 {
            let step = sequence.next_step().unwrap();
            assert_eq!(step.run_id, 77);
            assert_eq!(step.step, expected_step);
            assert_eq!(step.total, 3);
            commands.push(
                step.command_v6(|text| {
                    clipboard.push(text.to_owned());
                    Ok(())
                })
                .unwrap(),
            );
            sequence.acknowledge(77, expected_step).unwrap();
        }

        assert_eq!(clipboard, vec!["你好\nKivo"]);
        assert_eq!(
            commands,
            vec![
                format_paste_command(77, 1, 3),
                "DELAY 77 2 3 500\n".into(),
                "MEDIA 77 3 3 205\n".into(),
            ]
        );
        assert!(sequence.is_complete());
    }

    #[test]
    fn encodes_function_punctuation_and_numpad_keys() {
        assert_eq!(encode_hotkey(&["f1".into()]).unwrap(), chord(0, &[0x3a]));
        assert_eq!(encode_hotkey(&["f24".into()]).unwrap(), chord(0, &[0x73]));
        assert_eq!(
            encode_hotkey(&["shift".into(), "left_bracket".into()]).unwrap(),
            chord(0x02, &[0x2f])
        );
        assert_eq!(
            encode_hotkey(&["numpad_0".into()]).unwrap(),
            chord(0, &[0x62])
        );
        assert_eq!(
            encode_hotkey(&["numpad_add".into()]).unwrap(),
            chord(0, &[0x57])
        );
        assert_eq!(
            encode_hotkey(&["print_screen".into()]).unwrap(),
            chord(0, &[0x46])
        );
    }

    #[test]
    fn primary_modifier_resolves_for_the_host_platform() {
        let expected = if cfg!(target_os = "macos") {
            0x08
        } else {
            0x01
        };
        assert_eq!(
            encode_hotkey(&["primary".into(), "v".into()]).unwrap(),
            chord(expected, &[0x19])
        );
    }

    #[test]
    fn formats_advanced_action_commands() {
        let step = |action| ActionStep {
            run_id: 12,
            button: "A".into(),
            trigger: ActionTrigger::Press,
            step: 2,
            total: 4,
            action,
        };

        assert_eq!(
            step(ButtonAction::Delay { duration_ms: 250 })
                .command_legacy(|_| Ok(()))
                .unwrap(),
            "DELAY 12 2 4 250\n"
        );
        assert_eq!(
            step(ButtonAction::Media {
                command: MediaCommand::PlayPause,
            })
            .command_legacy(|_| Ok(()))
            .unwrap(),
            "MEDIA 12 2 4 205\n"
        );
        assert_eq!(
            step(ButtonAction::Open {
                target: "https://example.com".into(),
            })
            .command_legacy(|_| Ok(()))
            .unwrap(),
            "HOST 12 2 4\n"
        );
    }

    #[test]
    fn formats_v6_chord_command() {
        let step = ActionStep {
            run_id: 7,
            button: "A".into(),
            trigger: ActionTrigger::Press,
            step: 1,
            total: 1,
            action: ButtonAction::Hotkey {
                keys: vec!["right_cmd".into(), "a".into(), "b".into()],
            },
        };
        assert_eq!(
            step.command_v6(|_| Ok(())).unwrap(),
            "CHORD 7 1 1 128 2 4 5\n"
        );
    }

    #[test]
    fn legacy_hotkey_rejects_multi_key_and_modifier_only_chords() {
        let step = |keys| ActionStep {
            run_id: 7,
            button: "A".into(),
            trigger: ActionTrigger::Press,
            step: 1,
            total: 1,
            action: ButtonAction::Hotkey { keys },
        };

        assert!(
            step(vec!["a".into(), "b".into()])
                .command_legacy(|_| Ok(()))
                .is_err()
        );
        assert!(
            step(vec!["right_cmd".into()])
                .command_legacy(|_| Ok(()))
                .is_err()
        );
    }

    #[test]
    fn builds_matrix_topology_and_resolves_normalized_contact() {
        let model = device_profile();
        let hardware = model.hardware_profile("esp-primary").unwrap();
        let reported_pins = BTreeSet::from([1, 2, 12, 13]);
        assert_eq!(
            topology_commands(hardware, 7, &reported_pins).unwrap(),
            vec![
                "CONFIG_BEGIN 7 30\n",
                "CONFIG_MATRIX 7 0 2 1 2 2 12 13\n",
                "CONFIG_COMMIT 7\n",
            ]
        );
        assert_eq!(
            topology_commands(hardware, 7, &BTreeSet::from([1, 2, 12]))
                .unwrap_err()
                .code,
            "capability_mismatch"
        );
        assert_eq!(
            model.button_for(
                "esp-primary",
                &PhysicalInput::Contact {
                    source: 0,
                    pin_a: 12,
                    pin_b: 1,
                },
            ),
            Some("A")
        );
    }

    #[test]
    fn ssd1306_topology_commands_precede_input_commands() {
        assert_eq!(
            topology_commands(&ssd1306_hardware(), 7, &BTreeSet::from([4, 5, 6])).unwrap(),
            vec![
                "CONFIG_BEGIN 7 30\n",
                "CONFIG_OLED 7 4 5\n",
                "CONFIG_DIRECT 7 0 1 6\n",
                "CONFIG_COMMIT 7\n",
            ]
        );
    }

    #[test]
    fn ssd1306_topology_requires_both_reported_pins() {
        let error = topology_commands(&ssd1306_hardware(), 7, &BTreeSet::from([4, 6])).unwrap_err();

        assert_eq!(error.code, "capability_mismatch");
        assert_eq!(error.params.get("gpio").map(String::as_str), Some("5"));
    }

    #[test]
    fn ssd1306_topology_rejects_unsupported_boards() {
        let hardware = ssd1306_hardware_for("luatos-esp32s3-aio", 4, 5);

        let error = topology_commands(&hardware, 7, &BTreeSet::from([4, 5, 6])).unwrap_err();

        assert_eq!(error.code, "oled_not_supported");
    }

    #[test]
    fn ssd1306_topology_rejects_the_same_pin() {
        let hardware = ssd1306_hardware_for("vccgnd-yd-rp2040", 4, 4);

        let error = topology_commands(&hardware, 7, &BTreeSet::from([4, 6])).unwrap_err();

        assert_eq!(error.code, "gpio_used_by_multiple_sources");
    }

    #[test]
    fn encodes_hid_hotkeys() {
        assert_eq!(
            encode_hotkey(&["cmd".into(), "shift".into(), "k".into()]),
            Ok(chord(10, &[14]))
        );
        assert_eq!(encode_hotkey(&["page_down".into()]), Ok(chord(0, &[78])));
    }

    #[test]
    fn encodes_sided_modifiers_and_six_ordinary_keys() {
        let chord = encode_hotkey(
            &["left_cmd", "right_cmd", "a", "b", "c", "d", "e", "f"].map(str::to_owned),
        )
        .unwrap();

        assert_eq!(chord.modifier_mask, 0x88);
        assert_eq!(chord.keycodes, vec![0x04, 0x05, 0x06, 0x07, 0x08, 0x09]);
    }

    #[test]
    fn accepts_modifier_only_and_rejects_duplicate_usage_or_seventh_key() {
        assert_eq!(
            encode_hotkey(&["right_alt".into()]).unwrap().keycodes,
            Vec::<u8>::new()
        );
        assert!(encode_hotkey(&["a".into(), "A".into()]).is_err());
        assert!(encode_hotkey(&["a", "b", "c", "d", "e", "f", "g"].map(str::to_owned),).is_err());
    }

    #[test]
    fn formats_the_platform_paste_shortcut() {
        #[cfg(target_os = "macos")]
        assert_eq!(format_paste_command(9, 1, 2), "PASTE 9 1 2\n");

        #[cfg(target_os = "windows")]
        assert_eq!(format_paste_command(9, 1, 2), "HOTKEY 9 1 2 3 25\n");

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(format_paste_command(9, 1, 2), "HOTKEY 9 1 2 1 25\n");
    }

    #[test]
    fn encodes_backtick_hotkey() {
        assert_eq!(encode_hotkey(&["backtick".into()]), Ok(chord(0, &[0x35])));
    }

    #[test]
    fn rejects_malformed_hotkeys() {
        for keys in [
            vec!["cmd", "cmd", "k"],
            vec!["a", "A"],
            vec!["left_alt", "option", "k"],
            vec!["cmd", "unknown"],
        ] {
            assert!(
                encode_hotkey(&keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>())
                    .is_err(),
                "{keys:?} must be rejected"
            );
        }
    }

    fn chord(modifier_mask: u8, keycodes: &[u8]) -> EncodedChord {
        EncodedChord {
            modifier_mask,
            keycodes: keycodes.to_vec(),
        }
    }
}
