use crate::{error::AppError, hardware::BoardProfile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayPanel {
    Ssd1306_128x32,
    Sh1106_128x64,
}

impl DisplayPanel {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Ssd1306_128x32 => "ssd1306_128x32",
            Self::Sh1106_128x64 => "sh1106_128x64",
        }
    }

    pub const fn cpp_name(self) -> &'static str {
        match self {
            Self::Ssd1306_128x32 => "Ssd1306_128x32",
            Self::Sh1106_128x64 => "Sh1106_128x64",
        }
    }

    pub const fn height(self) -> u16 {
        match self {
            Self::Ssd1306_128x32 => 32,
            Self::Sh1106_128x64 => 64,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayConfig {
    pub panel: DisplayPanel,
    pub sda: u8,
    pub scl: u8,
    pub address: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlPanelConfig {
    Ec11ConfirmBack {
        confirm: u8,
        encoder_press: u8,
        encoder_a: u8,
        encoder_b: u8,
        back: u8,
    },
}

impl ControlPanelConfig {
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

pub fn wiring_pins(
    display: Option<&DisplayConfig>,
    controls: Option<&ControlPanelConfig>,
    board: &BoardProfile,
) -> Result<Vec<u8>, AppError> {
    let Some(display) = display else {
        return if controls.is_some() {
            Err(AppError::new("controls_require_display"))
        } else {
            Ok(Vec::new())
        };
    };
    if !board.supports_display {
        return Err(AppError::new("display_not_supported").with_param("board_profile", board.id));
    }
    if !(0x08..=0x77).contains(&display.address) {
        return Err(AppError::new("invalid_display_address"));
    }
    let pins = [display.sda, display.scl]
        .into_iter()
        .chain(controls.into_iter().flat_map(ControlPanelConfig::pins))
        .collect::<Vec<_>>();
    if pins.iter().copied().collect::<BTreeSet<_>>().len() != pins.len() {
        return Err(AppError::new("gpio_used_by_multiple_sources"));
    }
    Ok(pins)
}
