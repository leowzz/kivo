#[cfg(test)]
use crate::profile::{TriggerActions, TriggerSettings};
use crate::{
    display::{DrawOperation, SceneMode, SceneUpdate},
    hardware::board_by_id,
    profile::{ActionTrigger, ButtonAction, HardwareProfile, InputSource, MediaCommand},
    workspace::AppError,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub use crate::input::*;
const DISPLAY_WIDTH: u16 = 128;
const DISPLAY_HEIGHT: u16 = 64;
const DISPLAY_MAX_REGIONS: usize = 8;
const DISPLAY_MAX_OPERATIONS: usize = 24;
const DISPLAY_MAX_TEXT_BYTES: usize = 48;
const DISPLAY_MAX_FONT_ID: u8 = 2;
pub const PRODUCT_CHUNK_BYTES: usize = 144;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceMessage {
    Hello(HelloCapabilities),
    ProductInfo {
        product_version_id: Option<String>,
        schema_version: u16,
        length: usize,
        sha256: Option<String>,
    },
    ProductBegin {
        length: usize,
        sha256: String,
    },
    ProductChunk {
        sequence: usize,
        bytes: Vec<u8>,
    },
    ProductEnd {
        length: usize,
        sha256: String,
    },
    ProductError {
        code: String,
    },
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

pub(crate) use crate::handshake::{HelloCapabilities, is_hello_line, validate_hello};

pub fn parse_device(line: &str) -> Option<DeviceMessage> {
    if line.len() >= 255 {
        return None;
    }
    if is_hello_line(line) {
        return crate::handshake::parse_hello(line).map(DeviceMessage::Hello);
    }
    let parts = line.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["PRODUCT_INFO", product_version_id, schema, length, sha256] => {
            let length = length.parse().ok()?;
            let product_version_id =
                (*product_version_id != "-").then(|| (*product_version_id).to_owned());
            let sha256 = (*sha256 != "-").then(|| (*sha256).to_owned());
            if length > crate::product::MAX_PRODUCT_DEFINITION_BYTES
                || product_version_id.is_some() != sha256.is_some()
                || product_version_id.is_none() != (length == 0)
                || product_version_id
                    .as_deref()
                    .is_some_and(|id| !crate::product::valid_product_version_id(id))
                || sha256.as_deref().is_some_and(|sha| !valid_sha256(sha))
            {
                return None;
            }
            Some(DeviceMessage::ProductInfo {
                product_version_id,
                schema_version: schema.parse().ok()?,
                length,
                sha256,
            })
        }
        ["PRODUCT_BEGIN", length, sha256] => {
            let length = length.parse().ok()?;
            (length > 0
                && length <= crate::product::MAX_PRODUCT_DEFINITION_BYTES
                && valid_sha256(sha256))
            .then(|| DeviceMessage::ProductBegin {
                length,
                sha256: (*sha256).to_owned(),
            })
        }
        ["PRODUCT_CHUNK", sequence, encoded] => {
            let sequence = sequence.parse().ok()?;
            let bytes = STANDARD.decode(encoded).ok()?;
            (!bytes.is_empty() && bytes.len() <= PRODUCT_CHUNK_BYTES)
                .then_some(DeviceMessage::ProductChunk { sequence, bytes })
        }
        ["PRODUCT_END", length, sha256] => {
            let length = length.parse().ok()?;
            (length > 0
                && length <= crate::product::MAX_PRODUCT_DEFINITION_BYTES
                && valid_sha256(sha256))
            .then(|| DeviceMessage::ProductEnd {
                length,
                sha256: (*sha256).to_owned(),
            })
        }
        ["PRODUCT_ERROR", code]
            if code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_') =>
        {
            Some(DeviceMessage::ProductError {
                code: (*code).to_owned(),
            })
        }
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
                    if *font_id > DISPLAY_MAX_FONT_ID {
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

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub struct ProductDefinitionTransfer {
    expected_length: usize,
    expected_sha256: String,
    next_sequence: usize,
    bytes: Vec<u8>,
    began: bool,
}

impl ProductDefinitionTransfer {
    pub fn new(expected_length: usize, expected_sha256: String) -> Result<Self, AppError> {
        if expected_length == 0
            || expected_length > crate::product::MAX_PRODUCT_DEFINITION_BYTES
            || !valid_sha256(&expected_sha256)
        {
            return Err(AppError::new("invalid_product_info"));
        }
        Ok(Self {
            expected_length,
            expected_sha256,
            next_sequence: 0,
            bytes: Vec::with_capacity(expected_length),
            began: false,
        })
    }

    pub fn push(&mut self, message: DeviceMessage) -> Result<Option<Vec<u8>>, AppError> {
        match message {
            DeviceMessage::ProductBegin { length, sha256 }
                if !self.began
                    && length == self.expected_length
                    && sha256 == self.expected_sha256 =>
            {
                self.began = true;
                Ok(None)
            }
            DeviceMessage::ProductChunk { sequence, bytes }
                if self.began
                    && sequence == self.next_sequence
                    && self.bytes.len() + bytes.len() <= self.expected_length =>
            {
                self.next_sequence += 1;
                self.bytes.extend(bytes);
                Ok(None)
            }
            DeviceMessage::ProductEnd { length, sha256 }
                if self.began
                    && length == self.expected_length
                    && sha256 == self.expected_sha256
                    && self.bytes.len() == self.expected_length =>
            {
                if crate::product::sha256_hex(&self.bytes) != self.expected_sha256 {
                    return Err(AppError::new("product_definition_sha_mismatch"));
                }
                Ok(Some(std::mem::take(&mut self.bytes)))
            }
            DeviceMessage::ProductError { code } => {
                Err(AppError::new("product_read_failed").with_param("device_code", code))
            }
            _ => Err(AppError::new("invalid_product_transfer_sequence")),
        }
    }
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
    if hardware.ssd1306.is_some() && hardware.sh1106.is_some() {
        return Err(AppError::new("multiple_oled_displays"));
    }
    if let Some(ssd1306) = &hardware.ssd1306 {
        if !board.supports_oled {
            return Err(AppError::new("oled_not_supported").with_param("board_profile", board.id));
        }
        if ssd1306.sda == ssd1306.scl {
            return Err(AppError::new("gpio_used_by_multiple_sources")
                .with_param("gpio", ssd1306.sda.to_string()));
        }
        if let Some(control_panel) = &ssd1306.control_panel {
            let pins = control_panel.pins();
            let unique = pins
                .into_iter()
                .chain([ssd1306.sda, ssd1306.scl])
                .collect::<BTreeSet<_>>();
            if unique.len() != 7 {
                return Err(AppError::new("gpio_used_by_multiple_sources"));
            }
        }
    }
    if let Some(sh1106) = &hardware.sh1106 {
        if !board.supports_oled {
            return Err(AppError::new("oled_not_supported").with_param("board_profile", board.id));
        }
        if sh1106.sda == sh1106.scl {
            return Err(AppError::new("gpio_used_by_multiple_sources")
                .with_param("gpio", sh1106.sda.to_string()));
        }
        if let Some(control_panel) = &sh1106.control_panel {
            let pins = control_panel.pins();
            let unique = pins
                .into_iter()
                .chain([sh1106.sda, sh1106.scl])
                .collect::<BTreeSet<_>>();
            if unique.len() != 7 {
                return Err(AppError::new("gpio_used_by_multiple_sources"));
            }
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
        if let Some(control_panel) = &ssd1306.control_panel {
            let [confirm, encoder_press, encoder_a, encoder_b, back] = control_panel.pins();
            lines.push(format!(
                "CONFIG_OLED_CONTROL {revision} {confirm} {encoder_press} {encoder_a} {encoder_b} {back}\n"
            ));
        }
    }
    if let Some(sh1106) = &hardware.sh1106 {
        lines.push(format!(
            "CONFIG_SH1106 {revision} {} {}\n",
            sh1106.sda, sh1106.scl
        ));
        if let Some(control_panel) = &sh1106.control_panel {
            let [confirm, encoder_press, encoder_a, encoder_b, back] = control_panel.pins();
            lines.push(format!(
                "CONFIG_OLED_CONTROL {revision} {confirm} {encoder_press} {encoder_a} {encoder_b} {back}\n"
            ));
        }
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
            InputSource::FeatureSwitch { gpio, .. } => {
                lines.push(format!(
                    "CONFIG_DIRECT {revision} {source_index} 1 {gpio}\n"
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
            InputSource::FeatureSwitch { gpio, .. } => vec![*gpio],
        })
        .collect::<BTreeSet<_>>();
    if let Some(ssd1306) = &hardware.ssd1306 {
        pins.insert(ssd1306.sda);
        pins.insert(ssd1306.scl);
        if let Some(control_panel) = &ssd1306.control_panel {
            pins.extend(control_panel.pins());
        }
    }
    if let Some(sh1106) = &hardware.sh1106 {
        pins.insert(sh1106.sda);
        pins.insert(sh1106.scl);
        if let Some(control_panel) = &sh1106.control_panel {
            pins.extend(control_panel.pins());
        }
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

    pub fn awaiting_step(&self) -> Option<ActionStep> {
        let step = self.awaiting?;
        Some(ActionStep {
            run_id: self.run_id,
            button: self.button.clone(),
            trigger: self.trigger,
            step,
            total: u16::try_from(self.actions.len()).ok()?,
            action: self.actions.get(self.next)?.clone(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputState {
    Down,
    Up,
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
        profile::{
            DeviceProfile, HardwareProfile, InputSource, OledControlPanelConfig,
            PROFILE_SCHEMA_VERSION,
        },
    };
    use std::collections::{BTreeMap, BTreeSet};

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
            snapshot_metadata: None,
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![HardwareProfile {
                id: "esp-primary".into(),
                name: "ESP primary".into(),
                board_profile_id: "yd-esp32-s3".into(),
                debounce_ms: 30,
                ssd1306: None,
                sh1106: None,
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
        ssd1306_hardware_for("yd-rp2040", 4, 5)
    }

    fn sh1106_hardware() -> HardwareProfile {
        serde_yaml_ng::from_str(
            &serde_yaml_ng::to_string(&ssd1306_hardware())
                .unwrap()
                .replace("ssd1306:", "sh1106:"),
        )
        .unwrap()
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
        *font_id = 3;
        assert_eq!(
            display_commands(&unsupported_font).unwrap_err(),
            "display_font_unsupported"
        );
    }

    #[test]
    fn accepts_each_declared_display_font() {
        for supported_font_id in [0, 1, 2] {
            let mut update = display_update(2, 1, SceneMode::Delta, "4 RUN");
            let DrawOperation::Text { font_id, .. } = &mut update.regions[0].operations[1] else {
                unreachable!();
            };
            *font_id = supported_font_id;

            assert!(display_commands(&update).is_ok());
        }
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
        let message = parse_device("HELLO 4 rp2040 yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();
        assert_eq!(
            message,
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 4,
                controller_family_id: "rp2040".into(),
                board_profile_id: "yd-rp2040".into(),
                firmware_build_id: "0.1.0+gabc1234".into(),
                product_version_id: None,
                pins: vec![0, 11, 22],
            })
        );
        assert!(parse_device("HELLO 4 esp32s3 yd-esp32-s3 0.1.0+gabc1234 3 0 6 18",).is_some());
        assert!(parse_device(
            "HELLO 4 rp2040 yd-rp2040 0.1.0+gabc1234 23 0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22",
        )
        .is_some());
    }

    #[test]
    fn parses_and_validates_protocol_v4_identity_and_build() {
        let message = parse_device("HELLO 4 rp2040 yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();
        let DeviceMessage::Hello(hello) = message else {
            panic!("expected HELLO");
        };

        assert_eq!(hello.protocol, 4);
        assert!(validate_hello(board_by_id("yd-rp2040").unwrap(), &hello).is_ok());
    }

    #[test]
    fn validates_legacy_firmware_board_ids_against_canonical_yd_boards() {
        let DeviceMessage::Hello(esp) =
            parse_device("HELLO 8 esp32s3 luatos-esp32s3-aio legacy-build 3 0 10 47").unwrap()
        else {
            panic!("expected ESP32-S3 HELLO");
        };
        assert!(validate_hello(board_by_id("yd-esp32-s3").unwrap(), &esp).is_ok());

        let DeviceMessage::Hello(rp2040) =
            parse_device("HELLO 8 rp2040 vccgnd-yd-rp2040 legacy-build 3 0 11 22").unwrap()
        else {
            panic!("expected RP2040 HELLO");
        };
        assert!(validate_hello(board_by_id("yd-rp2040").unwrap(), &rp2040).is_ok());
    }

    #[test]
    fn parses_protocol_v3_hello_for_backward_compatibility() {
        let message = parse_device("HELLO 3 rp2040 yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();

        assert!(matches!(
            message,
            DeviceMessage::Hello(HelloCapabilities { protocol: 3, .. })
        ));
    }

    #[test]
    fn parses_protocol_v6_hello() {
        let message = parse_device("HELLO 6 rp2040 yd-rp2040 0.1.0+gabc1234 3 0 11 22").unwrap();

        assert!(matches!(
            message,
            DeviceMessage::Hello(HelloCapabilities { protocol: 6, .. })
        ));
    }

    #[test]
    fn parses_protocol_v5_v7_and_v8_hello_compatibility_fixtures() {
        for protocol in [5, 7, 8] {
            let message = parse_device(&format!(
                "HELLO {protocol} rp2040 yd-rp2040 build 3 0 11 22"
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
    fn parses_protocol_v9_hello_with_product_or_legacy_marker() {
        let product =
            parse_device("HELLO 9 rp2040 yd-rp2040 build key-rp-k1-r01 3 0 11 22").unwrap();
        assert!(matches!(
            product,
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 9,
                product_version_id: Some(ref id),
                ..
            }) if id == "key-rp-k1-r01"
        ));
        let generic = parse_device("HELLO 9 rp2040 yd-rp2040 build - 3 0 11 22").unwrap();
        assert!(matches!(
            generic,
            DeviceMessage::Hello(HelloCapabilities {
                protocol: 9,
                product_version_id: None,
                ..
            })
        ));
    }

    #[test]
    fn product_transfer_validates_chunks_order_length_and_sha() {
        let bytes = br#"{"schema_version":1}"#;
        let sha256 = crate::product::sha256_hex(bytes);
        let info = parse_device(&format!(
            "PRODUCT_INFO key-rp-k1-r01 1 {} {sha256}",
            bytes.len()
        ));
        assert!(matches!(
            info,
            Some(DeviceMessage::ProductInfo {
                schema_version: 1,
                length,
                ..
            }) if length == bytes.len()
        ));

        let mut transfer = ProductDefinitionTransfer::new(bytes.len(), sha256.clone()).unwrap();
        assert_eq!(
            transfer
                .push(DeviceMessage::ProductBegin {
                    length: bytes.len(),
                    sha256: sha256.clone(),
                })
                .unwrap(),
            None
        );
        transfer
            .push(DeviceMessage::ProductChunk {
                sequence: 0,
                bytes: bytes[..8].to_vec(),
            })
            .unwrap();
        transfer
            .push(DeviceMessage::ProductChunk {
                sequence: 1,
                bytes: bytes[8..].to_vec(),
            })
            .unwrap();
        assert_eq!(
            transfer
                .push(DeviceMessage::ProductEnd {
                    length: bytes.len(),
                    sha256,
                })
                .unwrap(),
            Some(bytes.to_vec())
        );

        let mut out_of_order =
            ProductDefinitionTransfer::new(1, crate::product::sha256_hex(b"x")).unwrap();
        out_of_order
            .push(DeviceMessage::ProductBegin {
                length: 1,
                sha256: crate::product::sha256_hex(b"x"),
            })
            .unwrap();
        assert_eq!(
            out_of_order
                .push(DeviceMessage::ProductChunk {
                    sequence: 1,
                    bytes: vec![b'x'],
                })
                .unwrap_err()
                .code,
            "invalid_product_transfer_sequence"
        );
        assert!(parse_device("PRODUCT_CHUNK 0 !!!").is_none());
        assert!(
            parse_device(&format!(
                "PRODUCT_CHUNK 0 {}",
                STANDARD.encode(vec![0; PRODUCT_CHUNK_BYTES + 1])
            ))
            .is_none()
        );
    }

    #[test]
    fn rejects_unsupported_or_malformed_hello_capabilities() {
        assert!(parse_device("HELLO 2 esp32s3 yd-esp32-s3 build 2 1 2").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040  3 2 0 1").is_none());
        assert!(parse_device("HELLO\t4\trp2040\tyd-rp2040\tbuild\t2\t0\t1").is_none());
        assert!(parse_device(" HELLO 4 rp2040 yd-rp2040 build 2 0 1").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 2 0 1 ").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 2 0 1\n").is_some());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 2 1").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 0").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 2 1 1").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 1 256").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 1 -1").is_none());
        assert!(parse_device("HELLO 4 rp2040 yd-rp2040 build 1 1 trailing").is_none());
        assert!(parse_device("STATE 9 DIRECT 6 DOWN trailing\n").is_none());
        assert!(parse_device("STATE 9 CONTACT 0 1 1 DOWN\n").is_none());
        assert!(parse_device("DONE 9 0\n").is_none());
        assert!(parse_device(&"x".repeat(256)).is_none());
    }

    #[test]
    fn validates_hello_against_the_classified_board() {
        let board = board_by_id("yd-rp2040").unwrap();
        let hello = HelloCapabilities {
            protocol: 4,
            controller_family_id: "rp2040".into(),
            board_profile_id: "yd-rp2040".into(),
            firmware_build_id: "test".into(),
            product_version_id: None,
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
        wrong_board.board_profile_id = "yd-esp32-s3".into();
        assert_eq!(
            validate_hello(board, &wrong_board).unwrap_err().code,
            "board_profile_mismatch"
        );

        let mut unsafe_pin = hello;
        unsafe_pin.pins = vec![24];
        assert_eq!(
            validate_hello(board, &unsafe_pin).unwrap_err().code,
            "capability_mismatch"
        );
    }

    #[test]
    fn validates_legacy_rp2040_safe_pin_capability_subset() {
        let board = board_by_id("yd-rp2040").unwrap();
        let hello = HelloCapabilities {
            protocol: 3,
            controller_family_id: "rp2040".into(),
            board_profile_id: "yd-rp2040".into(),
            firmware_build_id: "legacy".into(),
            product_version_id: None,
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
    fn feature_switch_topology_uses_a_direct_gpio_without_a_button_binding() {
        let mut model = device_profile();
        model
            .hardware_profiles
            .first_mut()
            .unwrap()
            .inputs
            .push(InputSource::FeatureSwitch {
                id: "mode".into(),
                name: "Mode switch".into(),
                gpio: 3,
                buttons: BTreeSet::from(["A".into()]),
            });
        let hardware = model.hardware_profile("esp-primary").unwrap();
        assert_eq!(
            topology_commands(hardware, 7, &BTreeSet::from([1, 2, 3, 12, 13])).unwrap(),
            vec![
                "CONFIG_BEGIN 7 30\n",
                "CONFIG_MATRIX 7 0 2 1 2 2 12 13\n",
                "CONFIG_DIRECT 7 1 1 3\n",
                "CONFIG_COMMIT 7\n",
            ]
        );
        assert_eq!(
            model.button_for("esp-primary", &PhysicalInput::Direct { gpio: 3 }),
            None
        );
    }

    #[test]
    fn ssd1306_topology_commands_remain_backward_compatible() {
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
    fn sh1106_uses_its_own_topology_command() {
        assert_eq!(
            topology_commands(&sh1106_hardware(), 7, &BTreeSet::from([4, 5, 6])).unwrap(),
            vec![
                "CONFIG_BEGIN 7 30\n",
                "CONFIG_SH1106 7 4 5\n",
                "CONFIG_DIRECT 7 0 1 6\n",
                "CONFIG_COMMIT 7\n",
            ]
        );
    }

    #[test]
    fn oled_control_panel_precedes_inputs_and_requires_all_reported_pins() {
        let mut hardware = sh1106_hardware();
        hardware.sh1106.as_mut().unwrap().control_panel =
            Some(OledControlPanelConfig::Ec11ConfirmBack {
                confirm: 19,
                encoder_press: 20,
                encoder_a: 21,
                encoder_b: 22,
                back: 26,
            });
        let pins = BTreeSet::from([4, 5, 6, 19, 20, 21, 22, 26]);

        assert_eq!(
            topology_commands(&hardware, 7, &pins).unwrap(),
            vec![
                "CONFIG_BEGIN 7 30\n",
                "CONFIG_SH1106 7 4 5\n",
                "CONFIG_OLED_CONTROL 7 19 20 21 22 26\n",
                "CONFIG_DIRECT 7 0 1 6\n",
                "CONFIG_COMMIT 7\n",
            ]
        );
        let missing = topology_commands(&hardware, 7, &BTreeSet::from([4, 5, 6, 19, 20, 21, 22]))
            .unwrap_err();
        assert_eq!(missing.code, "capability_mismatch");
        assert_eq!(missing.params.get("gpio").map(String::as_str), Some("26"));
    }

    #[test]
    fn ssd1306_topology_requires_both_reported_pins() {
        let error = topology_commands(&ssd1306_hardware(), 7, &BTreeSet::from([4, 6])).unwrap_err();

        assert_eq!(error.code, "capability_mismatch");
        assert_eq!(error.params.get("gpio").map(String::as_str), Some("5"));
    }

    #[test]
    fn ssd1306_topology_rejects_unsupported_boards() {
        let hardware = ssd1306_hardware_for("yd-esp32-s3", 4, 5);

        let error = topology_commands(&hardware, 7, &BTreeSet::from([4, 5, 6])).unwrap_err();

        assert_eq!(error.code, "oled_not_supported");
    }

    #[test]
    fn ssd1306_topology_rejects_the_same_pin() {
        let hardware = ssd1306_hardware_for("yd-rp2040", 4, 4);

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
