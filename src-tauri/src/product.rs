use crate::{
    model::ModelLayout,
    profile::{
        DeviceProfile, HardwareProfile, InputSource, PROFILE_SCHEMA_VERSION, TriggerSettings,
    },
    storage::atomic_write,
    workspace::AppError,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const PRODUCT_DEFINITION_SCHEMA_VERSION: u16 = 1;
pub const MAX_PRODUCT_DEFINITION_BYTES: usize = 64 * 1024;
const CAPABILITY_ORDER: &[&str] = &["mic", "spk", "disp", "enc", "encp"];

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductIdentity {
    pub display_name: String,
    pub family_id: String,
    pub variant_id: String,
    pub hardware_revision: u16,
    pub product_version_id: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductDefinition {
    pub schema_version: u16,
    pub product: ProductIdentity,
    pub layout: ModelLayout,
    pub hardware_profile: HardwareProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedProductDefinition {
    pub definition: ProductDefinition,
    pub json: String,
    pub sha256: String,
    pub byte_length: usize,
}

#[derive(Clone, Debug)]
pub struct ProductDefinitionCache {
    directory: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl ProductDefinitionCache {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn load(
        &self,
        sha256: &str,
        byte_length: usize,
        product_version_id: &str,
        board_profile_id: &str,
    ) -> Option<ProductDefinition> {
        if !valid_sha256(sha256) || byte_length > MAX_PRODUCT_DEFINITION_BYTES {
            return None;
        }
        let _guard = self.lock.lock().ok()?;
        let path = self.path(sha256);
        let bytes = fs::read(&path).ok()?;
        let definition = ProductDefinition::parse_json(&bytes).ok();
        let valid = definition.as_ref().is_some_and(|definition| {
            bytes.len() == byte_length
                && definition.product.product_version_id == product_version_id
                && crate::hardware::board_profile_ids_match(
                    &definition.hardware_profile.board_profile_id,
                    board_profile_id,
                )
                && definition.normalize().is_ok_and(|normalized| {
                    normalized.sha256 == sha256 && normalized.json.as_bytes() == bytes
                })
        });
        if valid {
            definition
        } else {
            let _ = fs::remove_file(path);
            None
        }
    }

    pub fn store(&self, normalized: &NormalizedProductDefinition) -> Result<(), AppError> {
        if !valid_sha256(&normalized.sha256)
            || normalized.byte_length != normalized.json.len()
            || normalized.byte_length > MAX_PRODUCT_DEFINITION_BYTES
        {
            return Err(AppError::new("invalid_product_definition_cache_entry"));
        }
        let _guard = self
            .lock
            .lock()
            .map_err(|_| AppError::new("product_definition_cache_unavailable"))?;
        fs::create_dir_all(&self.directory).map_err(|error| {
            AppError::new("create_product_definition_cache_failed").with_detail(error.to_string())
        })?;
        atomic_write(&self.path(&normalized.sha256), normalized.json.as_bytes()).map_err(|detail| {
            AppError::new("write_product_definition_cache_failed").with_detail(detail)
        })
    }

    fn path(&self, sha256: &str) -> PathBuf {
        self.directory.join(format!("{sha256}.json"))
    }
}

impl ProductDefinition {
    pub fn parse_yaml(bytes: &[u8]) -> Result<Self, AppError> {
        if bytes.len() > MAX_PRODUCT_DEFINITION_BYTES {
            return Err(AppError::new("product_definition_too_large"));
        }
        let definition: Self = serde_yaml_ng::from_slice(bytes).map_err(|error| {
            AppError::new("invalid_product_definition_yaml").with_detail(error.to_string())
        })?;
        definition.validate()?;
        Ok(definition)
    }

    pub fn parse_json(bytes: &[u8]) -> Result<Self, AppError> {
        if bytes.len() > MAX_PRODUCT_DEFINITION_BYTES {
            return Err(AppError::new("product_definition_too_large"));
        }
        let definition: Self = serde_json::from_slice(bytes).map_err(|error| {
            AppError::new("invalid_product_definition_json").with_detail(error.to_string())
        })?;
        definition.validate()?;
        Ok(definition)
    }

    pub fn load(path: &Path) -> Result<Self, AppError> {
        let bytes = fs::read(path).map_err(|error| {
            AppError::new("read_product_definition_failed").with_detail(error.to_string())
        })?;
        Self::parse_yaml(&bytes)
    }

    pub fn save_yaml(&self, path: &Path) -> Result<(), AppError> {
        self.validate()?;
        let yaml = serde_yaml_ng::to_string(self).map_err(|error| {
            AppError::new("serialize_product_definition_failed").with_detail(error.to_string())
        })?;
        atomic_write(path, yaml.as_bytes())
            .map_err(|error| AppError::new("save_product_definition_failed").with_detail(error))
    }

    pub fn normalize(&self) -> Result<NormalizedProductDefinition, AppError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| {
            AppError::new("serialize_product_definition_failed").with_detail(error.to_string())
        })?;
        if bytes.len() > MAX_PRODUCT_DEFINITION_BYTES {
            return Err(AppError::new("product_definition_too_large"));
        }
        let sha256 = hex_digest(&bytes);
        let json = String::from_utf8(bytes).expect("serde_json always emits UTF-8");
        Ok(NormalizedProductDefinition {
            definition: self.clone(),
            byte_length: json.len(),
            json,
            sha256,
        })
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.schema_version != PRODUCT_DEFINITION_SCHEMA_VERSION {
            return Err(AppError::new("unsupported_product_definition_schema"));
        }
        let controller_token =
            crate::hardware::product_id_token_for_board(&self.hardware_profile.board_profile_id)
                .ok_or_else(|| {
                    AppError::new("unknown_board_profile")
                        .with_param("board_profile", &self.hardware_profile.board_profile_id)
                })?;
        validate_product_identity(&self.product, controller_token, button_count(&self.layout))?;
        if (self.hardware_profile.ssd1306.is_some() || self.hardware_profile.sh1106.is_some())
            && !self
                .product
                .capabilities
                .iter()
                .any(|value| value == "disp")
        {
            return Err(AppError::new("display_capability_required"));
        }
        if (self
            .hardware_profile
            .ssd1306
            .as_ref()
            .and_then(|oled| oled.control_panel.as_ref())
            .is_some()
            || self
                .hardware_profile
                .sh1106
                .as_ref()
                .and_then(|oled| oled.control_panel.as_ref())
                .is_some())
            && !self
                .product
                .capabilities
                .iter()
                .any(|value| value == "encp")
        {
            return Err(AppError::new("encoder_press_capability_required"));
        }

        let profile = DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: self.layout.clone(),
            snapshot_metadata: None,
            trigger_settings: TriggerSettings::default(),
            hardware_profiles: vec![self.hardware_profile.clone()],
            actions: BTreeMap::new(),
        };
        profile.validate()?;
        Ok(())
    }

    pub fn as_runtime_profile(
        &self,
        trigger_settings: TriggerSettings,
        actions: BTreeMap<String, crate::profile::TriggerActions>,
    ) -> DeviceProfile {
        DeviceProfile {
            schema_version: PROFILE_SCHEMA_VERSION,
            profile: self.layout.clone(),
            snapshot_metadata: None,
            trigger_settings,
            hardware_profiles: vec![self.hardware_profile.clone()],
            actions,
        }
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_digest(bytes)
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn button_count(layout: &ModelLayout) -> usize {
    layout.groups.iter().map(|group| group.buttons.len()).sum()
}

fn validate_product_identity(
    identity: &ProductIdentity,
    controller_token: &str,
    key_count: usize,
) -> Result<(), AppError> {
    if identity.display_name.trim().is_empty() {
        return Err(AppError::new("invalid_product_display_name"));
    }
    if !valid_product_component(&identity.family_id) {
        return Err(AppError::new("invalid_product_family_id"));
    }
    if identity.hardware_revision == 0 {
        return Err(AppError::new("invalid_hardware_revision"));
    }

    let mut previous_order = 0usize;
    for (index, capability) in identity.capabilities.iter().enumerate() {
        let order = CAPABILITY_ORDER
            .iter()
            .position(|known| known == capability)
            .ok_or_else(|| {
                AppError::new("unknown_product_capability").with_param("capability", capability)
            })?
            + 1;
        if order <= previous_order {
            return Err(AppError::new("product_capabilities_not_canonical"));
        }
        previous_order = order;
        if index > 0 && identity.capabilities[index - 1] == *capability {
            return Err(AppError::new("duplicate_product_capability"));
        }
    }
    if identity.capabilities.iter().any(|value| value == "enc")
        && identity.capabilities.iter().any(|value| value == "encp")
    {
        return Err(AppError::new("conflicting_product_capabilities"));
    }

    let capability_suffix = identity
        .capabilities
        .iter()
        .map(|capability| format!("-{capability}"))
        .collect::<String>();
    let expected_variant = format!(
        "{}-{controller_token}-k{key_count}{capability_suffix}",
        identity.family_id
    );
    if identity.variant_id != expected_variant {
        return Err(AppError::new("product_variant_id_mismatch")
            .with_param("expected", expected_variant)
            .with_param("actual", &identity.variant_id));
    }
    let expected_version = format!("{}-r{:02}", identity.variant_id, identity.hardware_revision);
    if identity.product_version_id != expected_version {
        return Err(AppError::new("product_version_id_mismatch")
            .with_param("expected", expected_version)
            .with_param("actual", &identity.product_version_id));
    }
    Ok(())
}

pub fn valid_product_version_id(value: &str) -> bool {
    if !valid_product_component(value) {
        return false;
    }
    let Some((variant, revision)) = value.rsplit_once("-r") else {
        return false;
    };
    !variant.is_empty()
        && revision.len() >= 2
        && revision.bytes().all(|byte| byte.is_ascii_digit())
        && revision.parse::<u16>().is_ok_and(|revision| revision > 0)
}

fn valid_product_component(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_lowercase)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !value.contains("--")
}

pub fn generated_header(normalized: &NormalizedProductDefinition) -> Result<String, AppError> {
    let hardware = &normalized.definition.hardware_profile;
    let mut sources = Vec::new();
    for input in &hardware.inputs {
        match input {
            InputSource::Direct { keys, .. } if !keys.is_empty() => {
                let pins = keys
                    .values()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>();
                sources.push(GeneratedSource::Direct(pins.into_iter().collect()));
            }
            InputSource::ContactMatrix { keys, .. } if !keys.is_empty() => {
                let (rows, columns) = matrix_partitions(keys.values().copied());
                sources.push(GeneratedSource::Matrix(rows, columns));
            }
            InputSource::FeatureSwitch { gpio, .. } => {
                sources.push(GeneratedSource::Direct(vec![*gpio]));
            }
            InputSource::Direct { .. } | InputSource::ContactMatrix { .. } => {}
        }
    }

    let bytes = normalized
        .json
        .as_bytes()
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut topology = format!(
        "  TopologyBuilder builder(profile);\n  constexpr std::uint32_t revision = 1;\n  if (std::string_view(profile.boardProfileId) != \"{}\" ||\n      !builder.begin(revision, {})) return std::nullopt;\n",
        hardware.board_profile_id, hardware.debounce_ms
    );
    if let Some(oled) = &hardware.ssd1306 {
        topology.push_str(&format!(
            "  if (!builder.addOled(revision, {}, {})) return std::nullopt;\n",
            oled.sda, oled.scl
        ));
        if let Some(control_panel) = &oled.control_panel {
            let [confirm, encoder_press, encoder_a, encoder_b, back] = control_panel.pins();
            topology.push_str(&format!(
                "  if (!builder.addOledControlPanel(revision, {confirm}, {encoder_press}, {encoder_a}, {encoder_b}, {back})) return std::nullopt;\n"
            ));
        }
    }
    if let Some(oled) = &hardware.sh1106 {
        topology.push_str(&format!(
            "  if (!builder.addSh1106(revision, {}, {})) return std::nullopt;\n",
            oled.sda, oled.scl
        ));
        if let Some(control_panel) = &oled.control_panel {
            let [confirm, encoder_press, encoder_a, encoder_b, back] = control_panel.pins();
            topology.push_str(&format!(
                "  if (!builder.addOledControlPanel(revision, {confirm}, {encoder_press}, {encoder_a}, {encoder_b}, {back})) return std::nullopt;\n"
            ));
        }
    }
    for (index, source) in sources.iter().enumerate() {
        match source {
            GeneratedSource::Direct(pins) => topology.push_str(&format!(
                "  if (!builder.addDirect(revision, {index}, {{{}}})) return std::nullopt;\n",
                comma_pins(pins)
            )),
            GeneratedSource::Matrix(rows, columns) => topology.push_str(&format!(
                "  if (!builder.addMatrix(revision, {index}, {{{}}}, {{{}}})) return std::nullopt;\n",
                comma_pins(rows),
                comma_pins(columns)
            )),
        }
    }
    topology.push_str("  return builder.commit(revision);\n");

    Ok(format!(
        "#pragma once\n#include <cstddef>\n#include <cstdint>\n#include <optional>\n#include <string_view>\n#include \"InputTopology.h\"\n\ninline constexpr char kKivoProductVersionId[] = \"{}\";\ninline constexpr char kKivoProductDefinitionSha256[] = \"{}\";\ninline constexpr std::uint8_t kKivoProductDefinition[] = {{{bytes}}};\ninline constexpr std::size_t kKivoProductDefinitionSize = sizeof(kKivoProductDefinition);\n\ninline std::optional<RuntimeTopology> makeEmbeddedProductTopology(const BoardProfile &profile) {{\n{topology}}}\n",
        normalized.definition.product.product_version_id, normalized.sha256
    ))
}

enum GeneratedSource {
    Direct(Vec<u8>),
    Matrix(Vec<u8>, Vec<u8>),
}

fn matrix_partitions(pairs: impl IntoIterator<Item = [u8; 2]>) -> (Vec<u8>, Vec<u8>) {
    use std::collections::{BTreeMap, VecDeque};
    let mut neighbors = BTreeMap::<u8, Vec<u8>>::new();
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
        |(mut rows, mut columns), (pin, is_column)| {
            if is_column {
                columns.push(pin);
            } else {
                rows.push(pin);
            }
            (rows, columns)
        },
    )
}

fn comma_pins(pins: &[u8]) -> String {
    pins.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{ButtonDefinition, ButtonGroup},
        profile::{InputSource, OledControlPanelConfig, Sh1106Config, Ssd1306Config},
    };

    fn definition() -> ProductDefinition {
        ProductDefinition {
            schema_version: PRODUCT_DEFINITION_SCHEMA_VERSION,
            product: ProductIdentity {
                display_name: "Kivo Key 1".into(),
                family_id: "key".into(),
                variant_id: "key-rp-k1-disp".into(),
                hardware_revision: 1,
                product_version_id: "key-rp-k1-disp-r01".into(),
                capabilities: vec!["disp".into()],
            },
            layout: ModelLayout {
                id: "key-rp-k1-disp".into(),
                name: "Kivo Key 1".into(),
                groups: vec![ButtonGroup {
                    id: "keys".into(),
                    columns: 1,
                    buttons: vec![ButtonDefinition {
                        id: "K1".into(),
                        label: "K1".into(),
                    }],
                }],
            },
            hardware_profile: HardwareProfile {
                id: "hardware".into(),
                name: "Default hardware".into(),
                board_profile_id: "yd-rp2040".into(),
                debounce_ms: 30,
                ssd1306: None,
                sh1106: None,
                inputs: vec![InputSource::Direct {
                    id: "direct".into(),
                    keys: BTreeMap::from([("K1".into(), 1)]),
                }],
            },
        }
    }

    #[test]
    fn yaml_json_round_trip_and_digest_are_deterministic() {
        let definition = definition();
        let yaml = serde_yaml_ng::to_string(&definition).unwrap();
        let parsed = ProductDefinition::parse_yaml(yaml.as_bytes()).unwrap();
        let first = parsed.normalize().unwrap();
        let second = ProductDefinition::parse_json(first.json.as_bytes())
            .unwrap()
            .normalize()
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.sha256.len(), 64);
    }

    #[test]
    fn rejects_actions_and_inconsistent_identity() {
        let mut value = serde_yaml_ng::to_string(&definition()).unwrap();
        value.push_str("actions: {}\n");
        assert_eq!(
            ProductDefinition::parse_yaml(value.as_bytes())
                .unwrap_err()
                .code,
            "invalid_product_definition_yaml"
        );

        let mut mismatched = definition();
        mismatched.product.product_version_id = "key-rp-k2-disp-r01".into();
        assert_eq!(
            mismatched.validate().unwrap_err().code,
            "product_version_id_mismatch"
        );
    }

    #[test]
    fn controller_family_token_is_part_of_the_canonical_product_id() {
        let mut esp32s3 = definition();
        esp32s3.hardware_profile.board_profile_id = "yd-esp32-s3".into();
        esp32s3.product.variant_id = "key-s3-k1-disp".into();
        esp32s3.product.product_version_id = "key-s3-k1-disp-r01".into();
        esp32s3.layout.id = "key-s3-k1-disp".into();
        assert!(esp32s3.validate().is_ok());

        esp32s3.product.variant_id = "key-rp-k1-disp".into();
        assert_eq!(
            esp32s3.validate().unwrap_err().code,
            "product_variant_id_mismatch"
        );
    }

    #[test]
    fn generated_header_contains_definition_and_topology() {
        let normalized = definition().normalize().unwrap();
        let header = generated_header(&normalized).unwrap();
        assert!(header.contains("kKivoProductVersionId"));
        assert!(header.contains("builder.addDirect(revision, 0, {1})"));
        assert!(header.contains(&normalized.sha256));
    }

    #[test]
    fn generated_header_keeps_the_ssd1306_topology_path() {
        let mut definition = definition();
        definition.hardware_profile.ssd1306 = Some(Ssd1306Config {
            sda: 28,
            scl: 29,
            control_panel: None,
        });

        let header = generated_header(&definition.normalize().unwrap()).unwrap();

        assert!(header.contains("builder.addOled(revision, 28, 29)"));
        assert!(!header.contains("builder.addSh1106"));
    }

    #[test]
    fn oled_control_panel_owns_five_pins_without_changing_key_count() {
        let mut definition = definition();
        definition.product.capabilities = vec!["disp".into(), "encp".into()];
        definition.product.variant_id = "key-rp-k1-disp-encp".into();
        definition.product.product_version_id = "key-rp-k1-disp-encp-r01".into();
        definition.layout.id = "key-rp-k1-disp-encp".into();
        definition.hardware_profile.sh1106 = Some(Sh1106Config {
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

        let normalized = definition.normalize().unwrap();
        let header = generated_header(&normalized).unwrap();
        assert!(header.contains("builder.addSh1106(revision, 28, 29)"));
        assert!(header.contains("builder.addOledControlPanel(revision, 19, 20, 21, 22, 26)"));
        assert_eq!(button_count(&definition.layout), 1);

        let mut duplicate = definition.clone();
        duplicate.hardware_profile.sh1106 = Some(Sh1106Config {
            sda: 28,
            scl: 29,
            control_panel: Some(OledControlPanelConfig::Ec11ConfirmBack {
                confirm: 19,
                encoder_press: 19,
                encoder_a: 21,
                encoder_b: 22,
                back: 26,
            }),
        });
        assert_eq!(
            duplicate.validate().unwrap_err().code,
            "gpio_used_by_multiple_sources"
        );
    }

    #[test]
    fn cache_round_trips_canonical_definition_and_discards_corruption() {
        let directory = tempfile::tempdir().unwrap();
        let cache = ProductDefinitionCache::new(directory.path().into());
        let normalized = definition().normalize().unwrap();
        cache.store(&normalized).unwrap();

        assert_eq!(
            cache.load(
                &normalized.sha256,
                normalized.byte_length,
                "key-rp-k1-disp-r01",
                "yd-rp2040",
            ),
            Some(normalized.definition.clone())
        );

        let path = directory.path().join(format!("{}.json", normalized.sha256));
        fs::write(&path, b"{}").unwrap();
        assert!(
            cache
                .load(
                    &normalized.sha256,
                    normalized.byte_length,
                    "key-rp-k1-disp-r01",
                    "yd-rp2040",
                )
                .is_none()
        );
        assert!(!path.exists());
    }

    #[test]
    fn cache_rejects_identity_or_size_mismatch() {
        let directory = tempfile::tempdir().unwrap();
        let cache = ProductDefinitionCache::new(directory.path().into());
        let normalized = definition().normalize().unwrap();
        cache.store(&normalized).unwrap();
        assert!(
            cache
                .load(
                    &normalized.sha256,
                    normalized.byte_length + 1,
                    "key-rp-k1-disp-r01",
                    "yd-rp2040",
                )
                .is_none()
        );
    }
}
