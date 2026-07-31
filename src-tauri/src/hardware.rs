use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsbMode {
    Runtime,
    Bootloader,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsbIdentity {
    pub vid: u16,
    pub pid: u16,
    pub mode: UsbMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControllerFamily {
    pub id: &'static str,
    pub display_name: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardProfile {
    pub id: &'static str,
    pub family_id: &'static str,
    pub display_name: &'static str,
    pub runtime_usb: UsbIdentity,
    pub bootloader_usb: Option<UsbIdentity>,
    pub safe_pins: &'static [u8],
    pub firmware_environment: &'static str,
}

pub(crate) const ESP32S3_FAMILY_ID: &str = "esp32s3";
pub(crate) const RP2040_FAMILY_ID: &str = "rp2040";
pub(crate) const LUATOS_ESP32S3_AIO_BOARD_ID: &str = "luatos-esp32s3-aio";
pub(crate) const VCCGND_YD_RP2040_BOARD_ID: &str = "vccgnd-yd-rp2040";

const ESP32S3_SAFE_PINS: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18];
const YD_RP2040_SAFE_PINS: &[u8] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
];

pub const CONTROLLER_FAMILIES: &[ControllerFamily] = &[
    ControllerFamily {
        id: ESP32S3_FAMILY_ID,
        display_name: "ESP32-S3",
    },
    ControllerFamily {
        id: RP2040_FAMILY_ID,
        display_name: "RP2040",
    },
];

pub const BOARD_PROFILES: &[BoardProfile] = &[
    BoardProfile {
        id: LUATOS_ESP32S3_AIO_BOARD_ID,
        family_id: ESP32S3_FAMILY_ID,
        display_name: "LuatOS ESP32-S3 AIO",
        runtime_usb: UsbIdentity {
            vid: 0x303a,
            pid: 0x4002,
            mode: UsbMode::Runtime,
        },
        bootloader_usb: None,
        safe_pins: ESP32S3_SAFE_PINS,
        firmware_environment: "esp32s3",
    },
    BoardProfile {
        id: VCCGND_YD_RP2040_BOARD_ID,
        family_id: RP2040_FAMILY_ID,
        display_name: "VCC-GND YD-RP2040",
        runtime_usb: UsbIdentity {
            vid: 0x2e8a,
            pid: 0x102e,
            mode: UsbMode::Runtime,
        },
        bootloader_usb: Some(UsbIdentity {
            vid: 0x2e8a,
            pid: 0x0003,
            mode: UsbMode::Bootloader,
        }),
        safe_pins: YD_RP2040_SAFE_PINS,
        firmware_environment: "rp2040",
    },
];

#[derive(Clone, Copy, Debug)]
pub struct HardwareRegistry<'a> {
    families: &'a [ControllerFamily],
    boards: &'a [BoardProfile],
}

impl<'a> HardwareRegistry<'a> {
    pub const fn new(families: &'a [ControllerFamily], boards: &'a [BoardProfile]) -> Self {
        Self { families, boards }
    }

    pub fn board_by_runtime_usb(&self, vid: u16, pid: u16) -> Option<&'a BoardProfile> {
        self.registered_board(board_by_runtime_usb_in(self.boards, vid, pid)?)
    }

    pub fn board_by_bootloader_usb(&self, vid: u16, pid: u16) -> Option<&'a BoardProfile> {
        self.registered_board(board_by_bootloader_usb_in(self.boards, vid, pid)?)
    }

    pub fn board_by_id(&self, id: &str) -> Option<&'a BoardProfile> {
        self.registered_board(board_by_id_in(self.boards, id)?)
    }

    pub fn device_id(
        &self,
        board_profile_id: &str,
        hardware_serial: &str,
    ) -> Result<DeviceId, DeviceIdError> {
        device_id_from_boards(self.boards, board_profile_id, hardware_serial)
    }

    #[cfg(test)]
    fn is_valid(&self) -> bool {
        registries_are_valid(self.families, self.boards)
    }

    fn registered_board(&self, board: &'a BoardProfile) -> Option<&'a BoardProfile> {
        self.families
            .iter()
            .any(|family| family.id == board.family_id)
            .then_some(board)
    }
}

pub const fn compiled_registry() -> HardwareRegistry<'static> {
    HardwareRegistry::new(CONTROLLER_FAMILIES, BOARD_PROFILES)
}

#[cfg(test)]
pub(crate) const TEST_ESP32C3_FAMILY_ID: &str = "esp32c3";
#[cfg(test)]
pub(crate) const TEST_SECOND_RP2040_BOARD_ID: &str = "test-rp2040-board";
#[cfg(test)]
pub(crate) const TEST_ESP32C3_BOARD_ID: &str = "test-esp32c3-board";

#[cfg(test)]
const TEST_CONTROLLER_FAMILIES: &[ControllerFamily] = &[
    CONTROLLER_FAMILIES[0],
    CONTROLLER_FAMILIES[1],
    ControllerFamily {
        id: TEST_ESP32C3_FAMILY_ID,
        display_name: "ESP32-C3",
    },
];

#[cfg(test)]
const TEST_BOARD_PROFILES: &[BoardProfile] = &[
    BOARD_PROFILES[0],
    BOARD_PROFILES[1],
    BoardProfile {
        id: TEST_SECOND_RP2040_BOARD_ID,
        family_id: RP2040_FAMILY_ID,
        display_name: "Test RP2040 Board",
        runtime_usb: UsbIdentity {
            vid: 0x1209,
            pid: 0x2040,
            mode: UsbMode::Runtime,
        },
        bootloader_usb: Some(UsbIdentity {
            vid: 0x1209,
            pid: 0x2041,
            mode: UsbMode::Bootloader,
        }),
        safe_pins: &[0, 6, 22],
        firmware_environment: "test-rp2040",
    },
    BoardProfile {
        id: TEST_ESP32C3_BOARD_ID,
        family_id: TEST_ESP32C3_FAMILY_ID,
        display_name: "Test ESP32-C3 Board",
        runtime_usb: UsbIdentity {
            vid: 0x1209,
            pid: 0x32c3,
            mode: UsbMode::Runtime,
        },
        bootloader_usb: None,
        safe_pins: &[6],
        firmware_environment: "test-esp32c3",
    },
];

#[cfg(test)]
pub(crate) const fn test_registry() -> HardwareRegistry<'static> {
    HardwareRegistry::new(TEST_CONTROLLER_FAMILIES, TEST_BOARD_PROFILES)
}

pub fn board_by_runtime_usb(vid: u16, pid: u16) -> Option<&'static BoardProfile> {
    compiled_registry().board_by_runtime_usb(vid, pid)
}

pub fn board_by_bootloader_usb(vid: u16, pid: u16) -> Option<&'static BoardProfile> {
    compiled_registry().board_by_bootloader_usb(vid, pid)
}

pub fn board_by_id(id: &str) -> Option<&'static BoardProfile> {
    compiled_registry().board_by_id(id)
}

fn board_by_runtime_usb_in(boards: &[BoardProfile], vid: u16, pid: u16) -> Option<&BoardProfile> {
    boards.iter().find(|board| {
        board.runtime_usb.mode == UsbMode::Runtime
            && board.runtime_usb.vid == vid
            && board.runtime_usb.pid == pid
    })
}

fn board_by_bootloader_usb_in(
    boards: &[BoardProfile],
    vid: u16,
    pid: u16,
) -> Option<&BoardProfile> {
    boards.iter().find(|board| {
        board.bootloader_usb.is_some_and(|identity| {
            identity.mode == UsbMode::Bootloader && identity.vid == vid && identity.pid == pid
        })
    })
}

fn board_by_id_in<'a>(boards: &'a [BoardProfile], id: &str) -> Option<&'a BoardProfile> {
    boards.iter().find(|board| board.id == id)
}

#[cfg(test)]
fn registries_are_valid(families: &[ControllerFamily], boards: &[BoardProfile]) -> bool {
    let unique = |items: &[&str]| {
        items
            .iter()
            .enumerate()
            .all(|(index, item)| is_valid_component(item) && !items[index + 1..].contains(item))
    };
    let family_ids = families.iter().map(|family| family.id).collect::<Vec<_>>();
    let board_ids = boards.iter().map(|board| board.id).collect::<Vec<_>>();
    unique(&family_ids)
        && unique(&board_ids)
        && boards.iter().all(|board| {
            is_valid_component(board.family_id)
                && families.iter().any(|family| family.id == board.family_id)
                && board.runtime_usb.mode == UsbMode::Runtime
                && board
                    .bootloader_usb
                    .is_none_or(|identity| identity.mode == UsbMode::Bootloader)
                && !board.safe_pins.is_empty()
                && board
                    .safe_pins
                    .iter()
                    .enumerate()
                    .all(|(index, pin)| !board.safe_pins[index + 1..].contains(pin))
        })
        && boards.iter().enumerate().all(|(index, board)| {
            boards[index + 1..].iter().all(|other| {
                board.runtime_usb.vid != other.runtime_usb.vid
                    || board.runtime_usb.pid != other.runtime_usb.pid
            }) && board.bootloader_usb.is_none_or(|bootloader| {
                boards[index + 1..].iter().all(|other| {
                    other.bootloader_usb.is_none_or(|other_bootloader| {
                        bootloader.vid != other_bootloader.vid
                            || bootloader.pid != other_bootloader.pid
                    })
                })
            })
        })
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DeviceId {
    canonical: String,
    board_profile_id: String,
    hardware_serial: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceIdError;

impl fmt::Display for DeviceIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid device identity")
    }
}

impl std::error::Error for DeviceIdError {}

impl DeviceId {
    pub fn new(board_profile_id: &str, hardware_serial: &str) -> Result<Self, DeviceIdError> {
        compiled_registry().device_id(board_profile_id, hardware_serial)
    }

    pub fn parse(value: &str) -> Result<Self, DeviceIdError> {
        let (length, remainder) = value.split_once(':').ok_or(DeviceIdError)?;
        let board_length = length.parse::<usize>().map_err(|_| DeviceIdError)?;
        if length != board_length.to_string() || board_length == 0 {
            return Err(DeviceIdError);
        }
        let board_profile_id = remainder.get(..board_length).ok_or(DeviceIdError)?;
        let hardware_serial = remainder.get(board_length..).ok_or(DeviceIdError)?;
        let id = Self::new(board_profile_id, hardware_serial)?;
        (id.canonical == value).then_some(id).ok_or(DeviceIdError)
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    pub fn board_profile_id(&self) -> &str {
        &self.board_profile_id
    }

    pub fn hardware_serial(&self) -> &str {
        &self.hardware_serial
    }
}

fn device_id_from_boards(
    boards: &[BoardProfile],
    board_profile_id: &str,
    hardware_serial: &str,
) -> Result<DeviceId, DeviceIdError> {
    if !is_valid_component(board_profile_id)
        || !is_valid_component(hardware_serial)
        || board_by_id_in(boards, board_profile_id).is_none()
    {
        return Err(DeviceIdError);
    }
    Ok(DeviceId {
        canonical: format!(
            "{}:{}{}",
            board_profile_id.len(),
            board_profile_id,
            hardware_serial
        ),
        board_profile_id: board_profile_id.into(),
        hardware_serial: hardware_serial.into(),
    })
}

fn is_valid_component(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

impl Serialize for DeviceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for DeviceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const DUPLICATE_SAFE_PINS: &[u8] = &[0, 0];
    const EMPTY_SAFE_PINS: &[u8] = &[];

    #[test]
    fn registries_classify_modes_without_family_branches() {
        let esp = board_by_runtime_usb(0x303a, 0x4002).unwrap();
        assert_eq!(esp.family_id, "esp32s3");
        assert_eq!(esp.id, "luatos-esp32s3-aio");
        let rp = board_by_runtime_usb(0x2e8a, 0x102e).unwrap();
        assert_eq!(rp.family_id, "rp2040");
        assert_eq!(board_by_bootloader_usb(0x2e8a, 0x0003), Some(rp));
        assert!(board_by_runtime_usb(0x2e8a, 0x0003).is_none());
    }

    #[test]
    fn board_profiles_expose_only_the_approved_safe_pins() {
        assert_eq!(
            board_by_id("luatos-esp32s3-aio").unwrap().safe_pins,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13, 14, 15, 16, 17, 18]
        );
        assert_eq!(
            board_by_id("vccgnd-yd-rp2040").unwrap().safe_pins,
            (0..=22).collect::<Vec<_>>().as_slice()
        );
    }

    #[test]
    fn device_id_ignores_port_and_round_trips() {
        let id = DeviceId::new("vccgnd-yd-rp2040", "E0C9125B0D9B").unwrap();
        assert_eq!(DeviceId::parse(id.as_str()).unwrap(), id);
        assert_eq!(id.board_profile_id(), "vccgnd-yd-rp2040");
        assert_eq!(id.hardware_serial(), "E0C9125B0D9B");
    }

    #[test]
    fn injected_registry_supports_synthetic_extensions() {
        let registry = test_registry();
        let rp2040 = registry.board_by_id(TEST_SECOND_RP2040_BOARD_ID).unwrap();
        let esp32c3 = registry.board_by_id(TEST_ESP32C3_BOARD_ID).unwrap();

        assert_eq!(
            registry.board_by_runtime_usb(rp2040.runtime_usb.vid, rp2040.runtime_usb.pid),
            Some(rp2040)
        );
        let bootloader = rp2040.bootloader_usb.unwrap();
        assert_eq!(
            registry.board_by_bootloader_usb(bootloader.vid, bootloader.pid),
            Some(rp2040)
        );
        assert_eq!(registry.board_by_id(TEST_ESP32C3_BOARD_ID), Some(esp32c3));
        assert!(registry.is_valid());
        let id = registry
            .device_id(TEST_ESP32C3_BOARD_ID, "serial-c3")
            .unwrap();
        assert_eq!(
            id.as_str(),
            format!(
                "{}:{}{}",
                TEST_ESP32C3_BOARD_ID.len(),
                TEST_ESP32C3_BOARD_ID,
                "serial-c3"
            )
        );
    }

    #[test]
    fn registries_validate_static_entries_and_device_id_rejects_invalid_components() {
        assert!(registries_are_valid(CONTROLLER_FAMILIES, BOARD_PROFILES));
        assert!(DeviceId::new("unknown", "serial").is_err());
        assert!(DeviceId::new("vccgnd-yd-rp2040", " serial").is_err());
        assert!(DeviceId::parse("018:vccgnd-yd-rp2040serial").is_err());
    }

    #[test]
    fn registry_validation_rejects_invalid_extension_entries() {
        let mut families = [CONTROLLER_FAMILIES[0], CONTROLLER_FAMILIES[1]];
        families[1].id = families[0].id;
        assert!(!registries_are_valid(&families, BOARD_PROFILES));

        let mut families = [CONTROLLER_FAMILIES[0], CONTROLLER_FAMILIES[1]];
        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        families[0].id = "esp 32s3";
        boards[0].family_id = "esp 32s3";
        assert!(!registries_are_valid(&families, &boards));

        let mut families = [CONTROLLER_FAMILIES[0], CONTROLLER_FAMILIES[1]];
        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        families[0].id = "esp\u{0007}32s3";
        boards[0].family_id = "esp\u{0007}32s3";
        assert!(!registries_are_valid(&families, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].id = boards[0].id;
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].id = "yd rp2040";
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].id = "yd\u{0007}rp2040";
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].family_id = "unknown";
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].runtime_usb = boards[0].runtime_usb;
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].safe_pins = DUPLICATE_SAFE_PINS;
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].safe_pins = EMPTY_SAFE_PINS;
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[1].bootloader_usb = Some(UsbIdentity {
            vid: 0x2e8a,
            pid: 0x0003,
            mode: UsbMode::Runtime,
        });
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));

        let mut boards = [BOARD_PROFILES[0], BOARD_PROFILES[1]];
        boards[0].bootloader_usb = boards[1].bootloader_usb;
        assert!(!registries_are_valid(CONTROLLER_FAMILIES, &boards));
    }

    #[test]
    fn device_id_serializes_as_its_canonical_string() {
        let id = DeviceId::new("vccgnd-yd-rp2040", "E0C9125B0D9B").unwrap();
        let yaml = serde_yaml_ng::to_string(&BTreeMap::from([(id.clone(), true)])).unwrap();
        assert_eq!(yaml.trim(), "16:vccgnd-yd-rp2040E0C9125B0D9B: true");
        assert_eq!(
            serde_yaml_ng::from_str::<BTreeMap<DeviceId, bool>>(&yaml)
                .unwrap()
                .get(&id),
            Some(&true)
        );
    }
}
