use serde::Serialize;
use std::collections::BTreeSet;

pub const HOST_PROTOCOL_VERSION: u16 = 13;
pub const DISPLAY_PROTOCOL_VERSION: u16 = 7;
pub const DISPLAY_LARGE_FONT_PROTOCOL_VERSION: u16 = 8;
pub const ACTION_RUN_PROTOCOL_VERSION: u16 = 6;
pub const OLED_PROTOCOL_VERSION: u16 = 4;
pub const SH1106_PROTOCOL_VERSION: u16 = 11;
pub const OLED_CONTROL_PANEL_PROTOCOL_VERSION: u16 = 10;
pub const ADVANCED_ACTION_PROTOCOL_VERSION: u16 = 5;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PhysicalInput {
    Direct { gpio: u8 },
    Contact { source: u8, pin_a: u8, pin_b: u8 },
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
