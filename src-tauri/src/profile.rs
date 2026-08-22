use crate::{
    hardware::board_by_id,
    model::ModelLayout,
    protocol::{
        ACTION_RUN_PROTOCOL_VERSION, ADVANCED_ACTION_PROTOCOL_VERSION,
        OLED_CONTROL_PANEL_PROTOCOL_VERSION, OLED_PROTOCOL_VERSION, PhysicalInput,
        SH1106_PROTOCOL_VERSION, encode_hotkey,
    },
    workspace::AppError,
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use crate::model::{ButtonDefinition, ButtonGroup};

pub const PROFILE_SCHEMA_VERSION: u16 = 3;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreateDeviceProfileRequest {
    Clone {
        name: String,
        source_profile_id: String,
    },
    Blank {
        name: String,
        board_profile_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SnapshotMetadata {
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_device_name: Option<String>,
}

impl SnapshotMetadata {
    pub fn new() -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        Self {
            created_at,
            source_device_id: None,
            source_device_name: None,
        }
    }

    pub fn from_device(device_id: impl Into<String>, device_name: impl Into<String>) -> Self {
        Self {
            source_device_id: Some(device_id.into()),
            source_device_name: Some(device_name.into()),
            ..Self::new()
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    Paste { text: String },
    Hotkey { keys: Vec<String> },
    Delay { duration_ms: u32 },
    Media { command: MediaCommand },
    Open { target: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionTrigger {
    Press,
    Release,
    LongPress,
    DoublePress,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SwitchState {
    Open,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TriggerSettings {
    pub long_press_ms: u32,
    pub double_press_ms: u32,
}

impl Default for TriggerSettings {
    fn default() -> Self {
        Self {
            long_press_ms: 500,
            double_press_ms: 300,
        }
    }
}

impl<'de> Deserialize<'de> for TriggerSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RawTriggerSettings {
            #[serde(default = "default_long_press_ms")]
            long_press_ms: u32,
            #[serde(default = "default_double_press_ms")]
            double_press_ms: u32,
        }

        let raw = RawTriggerSettings::deserialize(deserializer)?;
        let settings = Self {
            long_press_ms: raw.long_press_ms,
            double_press_ms: raw.double_press_ms,
        };
        settings.validate().map_err(de::Error::custom)?;
        Ok(settings)
    }
}

impl TriggerSettings {
    fn validate(&self) -> Result<(), &'static str> {
        if !(100..=5_000).contains(&self.long_press_ms) {
            return Err("invalid_long_press_ms");
        }
        if !(100..=1_000).contains(&self.double_press_ms) {
            return Err("invalid_double_press_ms");
        }
        Ok(())
    }
}

fn default_long_press_ms() -> u32 {
    TriggerSettings::default().long_press_ms
}

fn default_double_press_ms() -> u32 {
    TriggerSettings::default().double_press_ms
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerActions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub press: Vec<ButtonAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release: Vec<ButtonAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub long_press: Vec<ButtonAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub double_press: Vec<ButtonAction>,
}

impl TriggerActions {
    pub fn press(actions: Vec<ButtonAction>) -> Self {
        Self {
            press: actions,
            ..Self::default()
        }
    }

    pub fn action_count(&self) -> usize {
        self.press.len() + self.release.len() + self.long_press.len() + self.double_press.len()
    }

    pub fn actions_for(&self, trigger: ActionTrigger) -> &[ButtonAction] {
        match trigger {
            ActionTrigger::Press => &self.press,
            ActionTrigger::Release => &self.release,
            ActionTrigger::LongPress => &self.long_press,
            ActionTrigger::DoublePress => &self.double_press,
        }
    }

    pub fn all(&self) -> impl Iterator<Item = &ButtonAction> {
        self.press
            .iter()
            .chain(&self.release)
            .chain(&self.long_press)
            .chain(&self.double_press)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommand {
    PlayPause,
    PreviousTrack,
    NextTrack,
    Stop,
    VolumeUp,
    VolumeDown,
    Mute,
}

pub const MAX_DELAY_MS: u32 = 60_000;
pub const MAX_OPEN_TARGET_LENGTH: usize = 2_048;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputSource {
    Direct {
        id: String,
        keys: BTreeMap<String, u8>,
    },
    ContactMatrix {
        id: String,
        pins: Vec<u8>,
        keys: BTreeMap<String, [u8; 2]>,
    },
    FeatureSwitch {
        id: String,
        name: String,
        gpio: u8,
        #[serde(default)]
        buttons: BTreeSet<String>,
    },
}

impl InputSource {
    fn id(&self) -> &str {
        match self {
            Self::Direct { id, .. }
            | Self::ContactMatrix { id, .. }
            | Self::FeatureSwitch { id, .. } => id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Ssd1306Config {
    pub sda: u8,
    pub scl: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_panel: Option<OledControlPanelConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Sh1106Config {
    pub sda: u8,
    pub scl: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_panel: Option<OledControlPanelConfig>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OledControlPanelConfig {
    Ec11ConfirmBack {
        confirm: u8,
        encoder_press: u8,
        encoder_a: u8,
        encoder_b: u8,
        back: u8,
    },
}

impl OledControlPanelConfig {
    pub fn pins(&self) -> [u8; 5] {
        match self {
            Self::Ec11ConfirmBack {
                confirm,
                encoder_press,
                encoder_a,
                encoder_b,
                back,
            } => [*confirm, *encoder_press, *encoder_a, *encoder_b, *back],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct HardwareProfile {
    pub id: String,
    pub name: String,
    pub board_profile_id: String,
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssd1306: Option<Ssd1306Config>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sh1106: Option<Sh1106Config>,
    #[serde(default)]
    pub inputs: Vec<InputSource>,
}

fn default_debounce_ms() -> u16 {
    30
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeviceProfile {
    pub schema_version: u16,
    pub profile: ModelLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_metadata: Option<SnapshotMetadata>,
    #[serde(default)]
    pub trigger_settings: TriggerSettings,
    #[serde(default)]
    pub hardware_profiles: Vec<HardwareProfile>,
    #[serde(default)]
    pub actions: BTreeMap<String, TriggerActions>,
}

pub fn blank_device_profile(id: String, name: String, board_profile_id: String) -> DeviceProfile {
    DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: ModelLayout {
            id,
            name,
            groups: Vec::new(),
        },
        snapshot_metadata: Some(SnapshotMetadata::new()),
        trigger_settings: TriggerSettings::default(),
        hardware_profiles: vec![HardwareProfile {
            id: "hardware".into(),
            name: "Default hardware".into(),
            board_profile_id,
            debounce_ms: default_debounce_ms(),
            ssd1306: None,
            sh1106: None,
            inputs: Vec::new(),
        }],
        actions: BTreeMap::new(),
    }
}

#[cfg(test)]
pub(crate) fn test_model_layout() -> ModelLayout {
    ModelLayout {
        id: "red-phone-v1".into(),
        name: "Red Phone v1".into(),
        groups: vec![ButtonGroup {
            id: "keys".into(),
            columns: 1,
            buttons: vec![ButtonDefinition {
                id: "UP".into(),
                label: "UP".into(),
            }],
        }],
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileChange {
    pub device_profile_id: String,
    pub host_mapping_changed: bool,
    pub topology_hardware_profile_ids: BTreeSet<String>,
}

impl ProfileChange {
    pub fn between(old: Option<&DeviceProfile>, new: Option<&DeviceProfile>) -> Self {
        let device_profile_id = new
            .or(old)
            .expect("a profile change requires an old or new profile")
            .profile
            .id
            .clone();
        let host_mapping_changed = match (old, new) {
            (Some(old), Some(new)) => old.profile != new.profile || old.actions != new.actions,
            (None, Some(_)) | (Some(_), None) => true,
            (None, None) => unreachable!("profile change has no source"),
        };
        let old_hardware = old
            .into_iter()
            .flat_map(|profile| &profile.hardware_profiles)
            .map(|hardware| (hardware.id.as_str(), hardware))
            .collect::<BTreeMap<_, _>>();
        let new_hardware = new
            .into_iter()
            .flat_map(|profile| &profile.hardware_profiles)
            .map(|hardware| (hardware.id.as_str(), hardware))
            .collect::<BTreeMap<_, _>>();
        let topology_hardware_profile_ids = old_hardware
            .keys()
            .chain(new_hardware.keys())
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|id| {
                topology_signature(old_hardware.get(id).copied())
                    != topology_signature(new_hardware.get(id).copied())
            })
            .map(str::to_owned)
            .collect();
        Self {
            device_profile_id,
            host_mapping_changed,
            topology_hardware_profile_ids,
        }
    }
}

#[derive(Eq, PartialEq)]
struct TopologySignature<'a> {
    board_profile_id: &'a str,
    debounce_ms: u16,
    ssd1306: Option<&'a Ssd1306Config>,
    sh1106: Option<&'a Sh1106Config>,
    inputs: &'a [InputSource],
}

fn topology_signature(hardware: Option<&HardwareProfile>) -> Option<TopologySignature<'_>> {
    hardware.map(|hardware| TopologySignature {
        board_profile_id: hardware.board_profile_id.as_str(),
        debounce_ms: hardware.debounce_ms,
        ssd1306: hardware.ssd1306.as_ref(),
        sh1106: hardware.sh1106.as_ref(),
        inputs: hardware.inputs.as_slice(),
    })
}

impl DeviceProfile {
    pub fn minimum_protocol_version(&self) -> u16 {
        let mut required = 3;
        if self
            .hardware_profiles
            .iter()
            .any(|hardware| hardware.ssd1306.is_some())
        {
            required = required.max(OLED_PROTOCOL_VERSION);
        }
        if self
            .hardware_profiles
            .iter()
            .any(|hardware| hardware.sh1106.is_some())
        {
            required = required.max(SH1106_PROTOCOL_VERSION);
        }
        if self.hardware_profiles.iter().any(|hardware| {
            hardware
                .ssd1306
                .as_ref()
                .and_then(|oled| oled.control_panel.as_ref())
                .is_some()
                || hardware
                    .sh1106
                    .as_ref()
                    .and_then(|oled| oled.control_panel.as_ref())
                    .is_some()
        }) {
            required = required.max(OLED_CONTROL_PANEL_PROTOCOL_VERSION);
        }
        for actions in self.actions.values() {
            if !actions.release.is_empty()
                || !actions.long_press.is_empty()
                || !actions.double_press.is_empty()
            {
                required = required.max(ACTION_RUN_PROTOCOL_VERSION);
            }
            for action in actions.all() {
                match action {
                    ButtonAction::Hotkey { keys } => {
                        if let Ok(chord) = encode_hotkey(keys)
                            && chord.keycodes.len() != 1
                        {
                            required = required.max(ACTION_RUN_PROTOCOL_VERSION);
                        }
                    }
                    ButtonAction::Delay { .. }
                    | ButtonAction::Media { .. }
                    | ButtonAction::Open { .. } => {
                        required = required.max(ADVANCED_ACTION_PROTOCOL_VERSION);
                    }
                    ButtonAction::Paste { .. } => {}
                }
            }
        }
        required
    }

    pub fn hardware_profile(&self, id: &str) -> Option<&HardwareProfile> {
        self.hardware_profiles
            .iter()
            .find(|hardware| hardware.id == id)
    }

    #[cfg(test)]
    pub fn compatible_hardware(&self, board_id: &str) -> Vec<&HardwareProfile> {
        self.hardware_profiles
            .iter()
            .filter(|hardware| hardware.board_profile_id == board_id)
            .collect()
    }

    pub fn button_for(&self, hardware_id: &str, input: &PhysicalInput) -> Option<&str> {
        let hardware = self.hardware_profile(hardware_id)?;
        let mut runtime_source = 0u8;
        for source in &hardware.inputs {
            match source {
                InputSource::Direct { keys, .. } if !keys.is_empty() => {
                    if let PhysicalInput::Direct { gpio } = input
                        && let Some((button, _)) = keys.iter().find(|(_, pin)| *pin == gpio)
                    {
                        return Some(button);
                    }
                    runtime_source = runtime_source.checked_add(1)?;
                }
                InputSource::ContactMatrix { keys, .. } if !keys.is_empty() => {
                    if let PhysicalInput::Contact {
                        source,
                        pin_a,
                        pin_b,
                    } = input
                        && *source == runtime_source
                    {
                        let pair = normalized_pair(*pin_a, *pin_b);
                        if let Some((button, _)) = keys
                            .iter()
                            .find(|(_, pins)| normalized_pair(pins[0], pins[1]) == pair)
                        {
                            return Some(button);
                        }
                    }
                    runtime_source = runtime_source.checked_add(1)?;
                }
                InputSource::FeatureSwitch { .. } => {
                    runtime_source = runtime_source.checked_add(1)?;
                }
                InputSource::Direct { .. } | InputSource::ContactMatrix { .. } => {}
            }
        }
        None
    }

    pub fn feature_switch_for(
        &self,
        hardware_id: &str,
        input: &PhysicalInput,
    ) -> Option<&InputSource> {
        let hardware = self.hardware_profile(hardware_id)?;
        hardware.inputs.iter().find(|source| {
            let InputSource::FeatureSwitch { gpio, .. } = source else {
                return false;
            };
            matches!(input, PhysicalInput::Direct { gpio: input_gpio } if input_gpio == gpio)
        })
    }

    pub fn button_is_enabled(
        &self,
        hardware_id: &str,
        button: &str,
        states: &BTreeMap<String, SwitchState>,
    ) -> bool {
        self.hardware_profile(hardware_id)
            .into_iter()
            .flat_map(|hardware| &hardware.inputs)
            .filter_map(|source| {
                let InputSource::FeatureSwitch { id, buttons, .. } = source else {
                    return None;
                };
                buttons.contains(button).then_some(id)
            })
            .all(|id| states.get(id) == Some(&SwitchState::Closed))
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(AppError::new("unsupported_profile_schema"));
        }
        self.profile
            .validate()
            .map_err(|detail| AppError::new("invalid_layout").with_param("detail", detail))?;
        self.trigger_settings.validate().map_err(AppError::new)?;

        let mut buttons = BTreeSet::new();
        for group in &self.profile.groups {
            if !valid_id(&group.id) {
                return Err(AppError::new("invalid_group_id").with_param("group", &group.id));
            }
            for button in &group.buttons {
                if !valid_id(&button.id) {
                    return Err(AppError::new("invalid_button_id").with_param("button", &button.id));
                }
                buttons.insert(button.id.as_str());
            }
        }
        let mut hardware_ids = BTreeSet::new();
        for hardware in &self.hardware_profiles {
            if !valid_id(&hardware.id) {
                return Err(AppError::new("invalid_hardware_profile_id")
                    .with_param("hardware_profile", &hardware.id));
            }
            if !hardware_ids.insert(hardware.id.as_str()) {
                return Err(AppError::new("duplicate_hardware_profile")
                    .with_param("hardware_profile", &hardware.id));
            }
            if hardware.name.trim().is_empty() {
                return Err(AppError::new("invalid_hardware_profile_name")
                    .with_param("hardware_profile", &hardware.id));
            }
            let board = board_by_id(&hardware.board_profile_id).ok_or_else(|| {
                AppError::new("unknown_board_profile")
                    .with_param("board_profile", &hardware.board_profile_id)
            })?;
            if !(1..=1000).contains(&hardware.debounce_ms) {
                return Err(
                    AppError::new("invalid_debounce").with_param("hardware_profile", &hardware.id)
                );
            }

            let mut source_ids = BTreeSet::new();
            let mut owned_pins = BTreeSet::new();
            let mut bound_buttons = BTreeSet::new();
            if hardware.ssd1306.is_some() && hardware.sh1106.is_some() {
                return Err(AppError::new("multiple_oled_displays")
                    .with_param("hardware_profile", &hardware.id));
            }
            if let Some(ssd1306) = &hardware.ssd1306 {
                validate_oled(
                    ssd1306.sda,
                    ssd1306.scl,
                    ssd1306.control_panel.as_ref(),
                    board.supports_oled,
                    board.id,
                    board.safe_pins,
                    &mut owned_pins,
                )?;
            }
            if let Some(sh1106) = &hardware.sh1106 {
                validate_oled(
                    sh1106.sda,
                    sh1106.scl,
                    sh1106.control_panel.as_ref(),
                    board.supports_oled,
                    board.id,
                    board.safe_pins,
                    &mut owned_pins,
                )?;
            }
            for source in &hardware.inputs {
                if !valid_id(source.id()) || !source_ids.insert(source.id()) {
                    return Err(AppError::new("invalid_input_source")
                        .with_param("hardware_profile", &hardware.id));
                }
                match source {
                    InputSource::Direct { keys, .. } => {
                        for (button, gpio) in keys {
                            validate_binding(button, &buttons, &mut bound_buttons)?;
                            validate_pin(*gpio, board.safe_pins, &mut owned_pins)?;
                        }
                    }
                    InputSource::ContactMatrix { pins, keys, .. } => {
                        let unique_pins = pins.iter().copied().collect::<BTreeSet<_>>();
                        if pins.len() < 2 || unique_pins.len() != pins.len() {
                            return Err(AppError::new("invalid_matrix_pins"));
                        }
                        for pin in pins {
                            validate_pin(*pin, board.safe_pins, &mut owned_pins)?;
                        }
                        let mut pairs = BTreeSet::new();
                        let mut edges = Vec::new();
                        for (button, pair) in keys {
                            validate_binding(button, &buttons, &mut bound_buttons)?;
                            let [left, right] = *pair;
                            if left == right
                                || !unique_pins.contains(&left)
                                || !unique_pins.contains(&right)
                            {
                                return Err(AppError::new("invalid_contact_pair"));
                            }
                            let pair = normalized_pair(left, right);
                            if !pairs.insert(pair) {
                                return Err(AppError::new("duplicate_contact_pair"));
                            }
                            edges.push(pair);
                        }
                        validate_bipartite(&edges)?;
                    }
                    InputSource::FeatureSwitch {
                        id: _,
                        name,
                        gpio,
                        buttons: gated_buttons,
                    } => {
                        if name.trim().is_empty() {
                            return Err(AppError::new("invalid_feature_switch_name")
                                .with_param("hardware_profile", &hardware.id));
                        }
                        validate_pin(*gpio, board.safe_pins, &mut owned_pins)?;
                        for button in gated_buttons {
                            if !buttons.contains(button.as_str()) {
                                return Err(AppError::new("unknown_feature_switch_button")
                                    .with_param("button", button));
                            }
                        }
                    }
                }
            }
        }

        for (button, actions) in &self.actions {
            if !buttons.contains(button.as_str()) {
                return Err(AppError::new("unknown_action_button").with_param("button", button));
            }
            for action in actions.all() {
                match action {
                    ButtonAction::Paste { text } if text.is_empty() => {
                        return Err(AppError::new("empty_paste_text").with_param("button", button));
                    }
                    ButtonAction::Hotkey { keys } => {
                        encode_hotkey(keys).map_err(|detail| {
                            AppError::new("invalid_hotkey")
                                .with_param("button", button)
                                .with_param("detail", detail)
                        })?;
                    }
                    ButtonAction::Delay { duration_ms }
                        if !(1..=MAX_DELAY_MS).contains(duration_ms) =>
                    {
                        return Err(AppError::new("invalid_delay")
                            .with_param("button", button)
                            .with_param("maximum", MAX_DELAY_MS.to_string()));
                    }
                    ButtonAction::Open { target }
                        if target.trim().is_empty()
                            || target.len() > MAX_OPEN_TARGET_LENGTH
                            || target.contains('\0') =>
                    {
                        return Err(
                            AppError::new("invalid_open_target").with_param("button", button)
                        );
                    }
                    ButtonAction::Paste { .. }
                    | ButtonAction::Delay { .. }
                    | ButtonAction::Media { .. }
                    | ButtonAction::Open { .. } => {}
                }
            }
        }
        Ok(())
    }

    pub fn uses_advanced_actions(&self) -> bool {
        self.actions
            .values()
            .flat_map(TriggerActions::all)
            .any(|action| {
                matches!(
                    action,
                    ButtonAction::Delay { .. }
                        | ButtonAction::Media { .. }
                        | ButtonAction::Open { .. }
                )
            })
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn validate_binding<'a>(
    button: &'a str,
    known: &BTreeSet<&str>,
    bound: &mut BTreeSet<&'a str>,
) -> Result<(), AppError> {
    if !known.contains(button) {
        return Err(AppError::new("unknown_hardware_button").with_param("button", button));
    }
    if !bound.insert(button) {
        return Err(AppError::new("button_bound_multiple_times").with_param("button", button));
    }
    Ok(())
}

fn validate_pin(pin: u8, safe_pins: &[u8], owned: &mut BTreeSet<u8>) -> Result<(), AppError> {
    if !safe_pins.contains(&pin) {
        return Err(AppError::new("unsupported_gpio").with_param("gpio", pin.to_string()));
    }
    if !owned.insert(pin) {
        return Err(
            AppError::new("gpio_used_by_multiple_sources").with_param("gpio", pin.to_string())
        );
    }
    Ok(())
}

fn validate_oled(
    sda: u8,
    scl: u8,
    control_panel: Option<&OledControlPanelConfig>,
    supported: bool,
    board_id: &str,
    safe_pins: &[u8],
    owned: &mut BTreeSet<u8>,
) -> Result<(), AppError> {
    if !supported {
        return Err(AppError::new("oled_not_supported").with_param("board_profile", board_id));
    }
    validate_pin(sda, safe_pins, owned)?;
    validate_pin(scl, safe_pins, owned)?;
    if let Some(control_panel) = control_panel {
        for pin in control_panel.pins() {
            validate_pin(pin, safe_pins, owned)?;
        }
    }
    Ok(())
}

fn normalized_pair(left: u8, right: u8) -> (u8, u8) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn validate_bipartite(edges: &[(u8, u8)]) -> Result<(), AppError> {
    let mut neighbors: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
    for &(left, right) in edges {
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
            for neighbor in &neighbors[&pin] {
                match colors.get(neighbor) {
                    Some(neighbor_color) if *neighbor_color == color => {
                        return Err(AppError::new("matrix_not_bipartite"));
                    }
                    Some(_) => {}
                    None => {
                        colors.insert(*neighbor, !color);
                        queue.push_back(*neighbor);
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml_profile(
        board_profile_id: &str,
        ssd1306: Option<(u8, u8)>,
        inputs: &str,
    ) -> DeviceProfile {
        let ssd1306 = ssd1306
            .map(|(sda, scl)| format!("    ssd1306:\n      sda: {sda}\n      scl: {scl}\n"))
            .unwrap_or_default();
        serde_yaml_ng::from_str(&format!(
            concat!(
                "schema_version: 3\n",
                "profile:\n",
                "  id: red-phone-v1\n",
                "  name: Phone\n",
                "  groups:\n",
                "    - id: keys\n",
                "      columns: 1\n",
                "      buttons:\n",
                "        - id: UP\n",
                "          label: UP\n",
                "hardware_profiles:\n",
                "  - id: hardware\n",
                "    name: Hardware\n",
                "    board_profile_id: {board_profile_id}\n",
                "    debounce_ms: 30\n",
                "{ssd1306}",
                "{inputs}\n",
                "actions: {{}}\n",
            ),
            board_profile_id = board_profile_id,
            ssd1306 = ssd1306,
            inputs = inputs,
        ))
        .unwrap()
    }

    fn profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: test_model_layout(),
            snapshot_metadata: None,
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![
                HardwareProfile {
                    id: "esp-primary".into(),
                    name: "ESP primary".into(),
                    board_profile_id: "yd-esp32-s3".into(),
                    debounce_ms: 30,
                    ssd1306: None,
                    sh1106: None,
                    inputs: vec![InputSource::Direct {
                        id: "direct".into(),
                        keys: BTreeMap::from([("UP".into(), 6)]),
                    }],
                },
                HardwareProfile {
                    id: "esp-secondary".into(),
                    name: "ESP secondary".into(),
                    board_profile_id: "yd-esp32-s3".into(),
                    debounce_ms: 30,
                    ssd1306: None,
                    sh1106: None,
                    inputs: vec![InputSource::Direct {
                        id: "direct".into(),
                        keys: BTreeMap::from([("UP".into(), 7)]),
                    }],
                },
            ],
            actions: BTreeMap::new(),
        }
    }

    #[test]
    fn schema_v3_defaults_trigger_settings_and_omits_empty_groups() {
        let profile: DeviceProfile = serde_yaml_ng::from_str(
            "schema_version: 3\nprofile:\n  id: pad\n  name: Pad\n  groups:\n    - id: keys\n      columns: 1\n      buttons:\n        - { id: A, label: A }\ntrigger_settings: {}\nhardware_profiles: []\nactions:\n  A:\n    press:\n      - { type: delay, duration_ms: 10 }\n",
        )
        .unwrap();

        assert_eq!(profile.trigger_settings, TriggerSettings::default());
        assert_eq!(profile.actions["A"].press.len(), 1);
        assert!(profile.actions["A"].release.is_empty());
        let yaml = serde_yaml_ng::to_string(&profile).unwrap();
        assert!(!yaml.contains("release:"));
    }

    #[test]
    fn trigger_timing_bounds_are_enforced() {
        let mut invalid_profile = profile();
        invalid_profile.trigger_settings.long_press_ms = 99;
        assert_eq!(
            invalid_profile.validate().unwrap_err().code,
            "invalid_long_press_ms"
        );
        invalid_profile.trigger_settings.long_press_ms = 500;
        invalid_profile.trigger_settings.double_press_ms = 1001;
        assert_eq!(
            invalid_profile.validate().unwrap_err().code,
            "invalid_double_press_ms"
        );

        let yaml = serde_yaml_ng::to_string(&profile())
            .unwrap()
            .replace("long_press_ms: 500", "long_press_ms: 99");
        assert!(serde_yaml_ng::from_str::<DeviceProfile>(&yaml).is_err());
    }

    #[test]
    fn rejects_removed_or_unknown_trigger_names() {
        let yaml = serde_yaml_ng::to_string(&profile())
            .unwrap()
            .replace("actions: {}", "actions:\n  UP:\n    short_press: []");

        assert!(serde_yaml_ng::from_str::<DeviceProfile>(&yaml).is_err());
    }

    #[test]
    fn live_update_classifies_action_only_changes_without_topology() {
        let old = profile();
        let mut new = old.clone();
        new.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Paste {
                text: "updated".into(),
            }]),
        );

        let change = ProfileChange::between(Some(&old), Some(&new));

        assert!(change.host_mapping_changed);
        assert!(change.topology_hardware_profile_ids.is_empty());
        assert_eq!(change.device_profile_id, "red-phone-v1");
    }

    #[test]
    fn validates_and_detects_advanced_actions() {
        let mut profile = profile();
        profile.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![
                ButtonAction::Delay { duration_ms: 200 },
                ButtonAction::Media {
                    command: MediaCommand::Mute,
                },
                ButtonAction::Open {
                    target: "https://example.com".into(),
                },
            ]),
        );

        assert!(profile.validate().is_ok());
        assert!(profile.uses_advanced_actions());

        profile.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Delay {
                duration_ms: MAX_DELAY_MS + 1,
            }]),
        );
        assert_eq!(profile.validate().unwrap_err().code, "invalid_delay");

        profile.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Open { target: " ".into() }]),
        );
        assert_eq!(profile.validate().unwrap_err().code, "invalid_open_target");
    }

    #[test]
    fn feature_switches_validate_targets_and_round_trip() {
        let mut profile = profile();
        profile.hardware_profiles[0]
            .inputs
            .push(InputSource::FeatureSwitch {
                id: "mode".into(),
                name: "Mode switch".into(),
                gpio: 7,
                buttons: BTreeSet::from(["UP".into()]),
            });

        profile.validate().unwrap();
        let yaml = serde_yaml_ng::to_string(&profile).unwrap();
        assert!(!yaml.contains("normal_state"));
        assert!(!yaml.contains("enabled_when"));
        let restored: DeviceProfile = serde_yaml_ng::from_str(&yaml).unwrap();
        assert_eq!(restored, profile);

        let mut legacy = serde_yaml_ng::to_value(&profile).unwrap();
        let feature_switch = legacy["hardware_profiles"][0]["inputs"][1]
            .as_mapping_mut()
            .unwrap();
        feature_switch.insert("normal_state".into(), "closed".into());
        feature_switch.insert("enabled_when".into(), "open".into());
        let restored_legacy: DeviceProfile = serde_yaml_ng::from_value(legacy).unwrap();
        assert_eq!(restored_legacy, profile);

        profile.hardware_profiles[0].inputs[1] = InputSource::FeatureSwitch {
            id: "mode".into(),
            name: "Mode switch".into(),
            gpio: 7,
            buttons: BTreeSet::from(["MISSING".into()]),
        };
        assert_eq!(
            profile.validate().unwrap_err().code,
            "unknown_feature_switch_button"
        );
    }

    #[test]
    fn profile_protocol_requirement_tracks_trigger_and_chord_features() {
        let mut press_only = profile();
        press_only.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["a".into()],
            }]),
        );
        assert_eq!(press_only.minimum_protocol_version(), 3);

        let mut release = press_only.clone();
        release.actions.insert(
            "UP".into(),
            TriggerActions {
                release: vec![ButtonAction::Paste {
                    text: "released".into(),
                }],
                ..TriggerActions::default()
            },
        );
        assert_eq!(release.minimum_protocol_version(), 6);

        let mut multi_key = press_only.clone();
        multi_key.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["a".into(), "b".into()],
            }]),
        );
        assert_eq!(multi_key.minimum_protocol_version(), 6);

        let mut modifier_only = press_only;
        modifier_only.actions.insert(
            "UP".into(),
            TriggerActions::press(vec![ButtonAction::Hotkey {
                keys: vec!["right_cmd".into()],
            }]),
        );
        assert_eq!(modifier_only.minimum_protocol_version(), 6);

        let mut control_panel = yaml_profile(
            "yd-rp2040",
            Some((28, 29)),
            "    inputs:\n      - type: direct\n        id: direct\n        keys:\n          UP: 6",
        );
        control_panel.hardware_profiles[0]
            .ssd1306
            .as_mut()
            .unwrap()
            .control_panel = Some(OledControlPanelConfig::Ec11ConfirmBack {
            confirm: 19,
            encoder_press: 20,
            encoder_a: 21,
            encoder_b: 22,
            back: 26,
        });
        assert!(control_panel.validate().is_ok());
        assert_eq!(
            control_panel.minimum_protocol_version(),
            OLED_CONTROL_PANEL_PROTOCOL_VERSION
        );
    }

    #[test]
    fn live_update_classifies_only_the_changed_hardware_topology() {
        let old = profile();
        let mut new = old.clone();
        new.hardware_profiles[1].debounce_ms = 45;

        let change = ProfileChange::between(Some(&old), Some(&new));

        assert!(!change.host_mapping_changed);
        assert_eq!(
            change.topology_hardware_profile_ids,
            BTreeSet::from(["esp-secondary".to_owned()])
        );
    }

    #[test]
    fn ssd1306_accepts_any_two_distinct_safe_pins_on_supported_board() {
        let profile = yaml_profile("yd-rp2040", Some((0, 22)), "    inputs: []");

        assert!(profile.validate().is_ok());
    }

    #[test]
    fn direct_input_accepts_rp2040_gpio_23() {
        let profile = yaml_profile(
            "yd-rp2040",
            None,
            "    inputs:\n      - type: direct\n        id: direct\n        keys:\n          UP: 23",
        );

        assert!(profile.validate().is_ok());
    }

    #[test]
    fn contact_matrix_accepts_new_rp2040_safe_gpios() {
        let profile = yaml_profile(
            "yd-rp2040",
            None,
            "    inputs:\n      - type: contact_matrix\n        id: matrix\n        pins: [26, 27]\n        keys:\n          UP: [26, 27]",
        );

        assert!(profile.validate().is_ok());
    }

    #[test]
    fn ssd1306_accepts_gpio_28_and_29() {
        let profile = yaml_profile("yd-rp2040", Some((28, 29)), "    inputs: []");

        assert!(profile.validate().is_ok());
    }

    #[test]
    fn rp2040_gpio_24_and_25_remain_unsupported() {
        for gpio in 24..=25 {
            let inputs = format!(
                "    inputs:\n      - type: direct\n        id: direct\n        keys:\n          UP: {gpio}"
            );
            let profile = yaml_profile("yd-rp2040", None, &inputs);

            let error = profile.validate().unwrap_err();
            assert_eq!(error.code, "unsupported_gpio");
            assert_eq!(error.params.get("gpio"), Some(&gpio.to_string()));
        }
    }

    #[test]
    fn ssd1306_rejects_the_same_pin_for_sda_and_scl() {
        let profile = yaml_profile("yd-rp2040", Some((4, 4)), "    inputs: []");

        assert_eq!(
            profile.validate().unwrap_err().code,
            "gpio_used_by_multiple_sources"
        );
    }

    #[test]
    fn ssd1306_rejects_unsupported_boards_before_pin_validation() {
        let profile = yaml_profile("yd-esp32-s3", Some((23, 24)), "    inputs: []");

        assert_eq!(profile.validate().unwrap_err().code, "oled_not_supported");
    }

    #[test]
    fn ssd1306_rejects_unsafe_pins() {
        let profile = yaml_profile("yd-rp2040", Some((24, 5)), "    inputs: []");

        assert_eq!(profile.validate().unwrap_err().code, "unsupported_gpio");
    }

    #[test]
    fn ssd1306_rejects_direct_input_pin_conflicts() {
        let profile = yaml_profile(
            "yd-rp2040",
            Some((4, 5)),
            "    inputs:\n      - type: direct\n        id: direct\n        keys:\n          UP: 4",
        );

        assert_eq!(
            profile.validate().unwrap_err().code,
            "gpio_used_by_multiple_sources"
        );
    }

    #[test]
    fn ssd1306_rejects_matrix_pin_conflicts() {
        let profile = yaml_profile(
            "yd-rp2040",
            Some((4, 5)),
            "    inputs:\n      - type: contact_matrix\n        id: matrix\n        pins: [1, 5]\n        keys:\n          UP: [1, 5]",
        );

        assert_eq!(
            profile.validate().unwrap_err().code,
            "gpio_used_by_multiple_sources"
        );
    }

    #[test]
    fn ssd1306_yaml_is_backward_compatible_when_omitted() {
        let profile = yaml_profile("yd-rp2040", None, "    inputs: []");

        assert!(profile.validate().is_ok());
        assert!(
            !serde_yaml_ng::to_string(&profile)
                .unwrap()
                .contains("ssd1306:")
        );
    }

    #[test]
    fn ssd1306_yaml_round_trips_when_configured() {
        let profile = yaml_profile("yd-rp2040", Some((4, 5)), "    inputs: []");

        let serialized = serde_yaml_ng::to_string(&profile).unwrap();
        let deserialized: DeviceProfile = serde_yaml_ng::from_str(&serialized).unwrap();

        assert!(serialized.contains("ssd1306:"));
        assert!(serialized.contains("sda: 4"));
        assert!(serialized.contains("scl: 5"));
        assert_eq!(deserialized, profile);
    }

    #[test]
    fn sh1106_round_trips_separately_and_requires_protocol_eleven() {
        let mut profile = yaml_profile("yd-rp2040", None, "    inputs: []");
        profile.hardware_profiles[0].sh1106 = Some(Sh1106Config {
            sda: 28,
            scl: 29,
            control_panel: Some(OledControlPanelConfig::Ec11ConfirmBack {
                confirm: 19,
                encoder_press: 20,
                encoder_a: 21,
                encoder_b: 22,
                back: 26,
            }),
        });

        profile.validate().unwrap();
        let serialized = serde_yaml_ng::to_string(&profile).unwrap();
        let restored: DeviceProfile = serde_yaml_ng::from_str(&serialized).unwrap();

        assert!(serialized.contains("sh1106:"));
        assert!(!serialized.contains("ssd1306:"));
        assert_eq!(profile.minimum_protocol_version(), SH1106_PROTOCOL_VERSION);
        assert_eq!(restored, profile);
    }

    #[test]
    fn hardware_profile_rejects_ssd1306_and_sh1106_together() {
        let mut profile = yaml_profile("yd-rp2040", Some((4, 5)), "    inputs: []");
        profile.hardware_profiles[0].sh1106 = Some(Sh1106Config {
            sda: 28,
            scl: 29,
            control_panel: None,
        });

        assert_eq!(
            profile.validate().unwrap_err().code,
            "multiple_oled_displays"
        );
    }

    #[test]
    fn ssd1306_changes_are_topology_changes() {
        let old = yaml_profile("yd-rp2040", None, "    inputs: []");
        let new = yaml_profile("yd-rp2040", Some((4, 5)), "    inputs: []");

        let change = ProfileChange::between(Some(&old), Some(&new));

        assert_eq!(
            change.topology_hardware_profile_ids,
            BTreeSet::from(["hardware".to_owned()])
        );
    }
}
