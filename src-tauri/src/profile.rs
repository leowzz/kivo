use crate::{
    hardware::board_by_id,
    model::ModelLayout,
    protocol::{PhysicalInput, encode_hotkey},
    workspace::AppError,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const PROFILE_SCHEMA_VERSION: u16 = 2;

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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ButtonAction {
    Paste { text: String },
    Hotkey { keys: Vec<String> },
}

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
}

impl InputSource {
    fn id(&self) -> &str {
        match self {
            Self::Direct { id, .. } | Self::ContactMatrix { id, .. } => id,
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
    #[serde(default)]
    pub hardware_profiles: Vec<HardwareProfile>,
    #[serde(default)]
    pub actions: BTreeMap<String, Vec<ButtonAction>>,
}

pub fn blank_device_profile(
    id: String,
    name: String,
    board_profile_id: String,
) -> DeviceProfile {
    DeviceProfile {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: ModelLayout {
            id,
            name,
            groups: Vec::new(),
        },
        hardware_profiles: vec![HardwareProfile {
            id: "hardware".into(),
            name: "Default hardware".into(),
            board_profile_id,
            debounce_ms: default_debounce_ms(),
            inputs: Vec::new(),
        }],
        actions: BTreeMap::new(),
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

fn topology_signature(hardware: Option<&HardwareProfile>) -> Option<(&str, u16, &[InputSource])> {
    hardware.map(|hardware| {
        (
            hardware.board_profile_id.as_str(),
            hardware.debounce_ms,
            hardware.inputs.as_slice(),
        )
    })
}

impl DeviceProfile {
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
                InputSource::Direct { .. } | InputSource::ContactMatrix { .. } => {}
            }
        }
        None
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(AppError::new("unsupported_profile_schema"));
        }
        self.profile
            .validate()
            .map_err(|detail| AppError::new("invalid_layout").with_param("detail", detail))?;

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
                }
            }
        }

        for (button, actions) in &self.actions {
            if !buttons.contains(button.as_str()) {
                return Err(AppError::new("unknown_action_button").with_param("button", button));
            }
            for action in actions {
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
                    ButtonAction::Paste { .. } => {}
                }
            }
        }
        Ok(())
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

    fn profile() -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: serde_json::from_str(include_str!("../../models/red-phone-v1.json")).unwrap(),
            hardware_profiles: vec![
                HardwareProfile {
                    id: "esp-primary".into(),
                    name: "ESP primary".into(),
                    board_profile_id: "luatos-esp32s3-aio".into(),
                    debounce_ms: 30,
                    inputs: vec![InputSource::Direct {
                        id: "direct".into(),
                        keys: BTreeMap::from([("UP".into(), 6)]),
                    }],
                },
                HardwareProfile {
                    id: "esp-secondary".into(),
                    name: "ESP secondary".into(),
                    board_profile_id: "luatos-esp32s3-aio".into(),
                    debounce_ms: 30,
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
    fn live_update_classifies_action_only_changes_without_topology() {
        let old = profile();
        let mut new = old.clone();
        new.actions.insert(
            "UP".into(),
            vec![ButtonAction::Paste {
                text: "updated".into(),
            }],
        );

        let change = ProfileChange::between(Some(&old), Some(&new));

        assert!(change.host_mapping_changed);
        assert!(change.topology_hardware_profile_ids.is_empty());
        assert_eq!(change.device_profile_id, "red-phone-v1");
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
}
