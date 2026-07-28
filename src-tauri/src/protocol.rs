use crate::config::ButtonAction;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Press {
    pub event_id: u64,
    pub gpio: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputState {
    Down,
    Up,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub event_id: u64,
    pub gpio: u8,
    pub state: InputState,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Reply {
    pub line: String,
    pub message: String,
}

pub fn parse_input(line: &str) -> Option<InputEvent> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "STATE" {
        return None;
    }
    let event_id = parts.next()?.parse().ok()?;
    let gpio = parts.next()?.parse().ok()?;
    let state = match parts.next()? {
        "DOWN" => InputState::Down,
        "UP" => InputState::Up,
        _ => return None,
    };
    parts.next().is_none().then_some(InputEvent {
        event_id,
        gpio,
        state,
    })
}

pub fn encode_hotkey(keys: &[String]) -> Result<(u8, u8), String> {
    let mut modifiers = 0;
    let mut keycode = None;
    for key in keys {
        let key = key.to_ascii_lowercase();
        let modifier = match key.as_str() {
            "ctrl" => Some(0x01),
            "shift" => Some(0x02),
            "alt" | "option" => Some(0x04),
            "cmd" => Some(0x08),
            _ => None,
        };
        if let Some(modifier) = modifier {
            if modifiers & modifier != 0 {
                return Err(format!("duplicate modifier {key}"));
            }
            modifiers |= modifier;
            continue;
        }
        let code = match key.as_bytes() {
            [letter @ b'a'..=b'z'] => letter - b'a' + 0x04,
            [digit @ b'1'..=b'9'] => digit - b'1' + 0x1e,
            b"0" => 0x27,
            b"enter" => 0x28,
            b"escape" => 0x29,
            b"backspace" => 0x2a,
            b"tab" => 0x2b,
            b"space" => 0x2c,
            b"home" => 0x4a,
            b"pageup" | b"page_up" => 0x4b,
            b"delete" => 0x4c,
            b"end" => 0x4d,
            b"pagedown" | b"page_down" => 0x4e,
            b"right" => 0x4f,
            b"left" => 0x50,
            b"down" => 0x51,
            b"up" => 0x52,
            _ => return Err(format!("unknown key {key}")),
        };
        if keycode.replace(code).is_some() {
            return Err("hotkey must have exactly one ordinary key".into());
        }
    }
    keycode
        .map(|keycode| (modifiers, keycode))
        .ok_or_else(|| "hotkey must have exactly one ordinary key".into())
}

pub fn reply(
    press: Press,
    action: Option<ButtonAction>,
    copy: impl FnOnce(&str) -> Result<(), String>,
) -> Reply {
    match action {
        Some(ButtonAction::Paste { text }) => match copy(&text) {
            Ok(()) => Reply {
                line: if cfg!(target_os = "macos") {
                    format!("PASTE {}\n", press.event_id)
                } else {
                    format!("HOTKEY {} 1 25\n", press.event_id)
                },
                message: format!("GPIO{}: PASTE {}", press.gpio, press.event_id),
            },
            Err(error) => Reply {
                line: format!("SKIP {}\n", press.event_id),
                message: format!(
                    "GPIO{}: SKIP {} (clipboard: {error})",
                    press.gpio, press.event_id
                ),
            },
        },
        Some(ButtonAction::Hotkey { keys }) => match encode_hotkey(&keys) {
            Ok((modifiers, keycode)) => Reply {
                line: format!("HOTKEY {} {modifiers} {keycode}\n", press.event_id),
                message: format!("GPIO{}: HOTKEY {}", press.gpio, press.event_id),
            },
            Err(error) => Reply {
                line: format!("SKIP {}\n", press.event_id),
                message: format!(
                    "GPIO{}: SKIP {} (hotkey: {error})",
                    press.gpio, press.event_id
                ),
            },
        },
        None => Reply {
            line: format!("SKIP {}\n", press.event_id),
            message: format!("GPIO{}: SKIP {}", press.gpio, press.event_id),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_complete_input_state_lines() {
        assert_eq!(
            parse_input("STATE 12 6 DOWN\n"),
            Some(InputEvent {
                event_id: 12,
                gpio: 6,
                state: InputState::Down,
            })
        );
        assert_eq!(
            parse_input("STATE 13 6 UP\n"),
            Some(InputEvent {
                event_id: 13,
                gpio: 6,
                state: InputState::Up,
            })
        );
        assert_eq!(parse_input("STATE nope 6 DOWN\n"), None);
        assert_eq!(parse_input("STATE 12 6 HELD\n"), None);
        assert_eq!(parse_input("STATE 12 6 DOWN extra\n"), None);
        assert_eq!(parse_input("PRESS 12 6\n"), None);
    }

    #[test]
    fn paste_action_uses_native_device_paste_on_macos() {
        let mut copied = String::new();

        let response = reply(
            Press {
                event_id: 12,
                gpio: 6,
            },
            Some(ButtonAction::Paste {
                text: "中文\nsecond".into(),
            }),
            |text| {
                copied = text.to_owned();
                Ok(())
            },
        );

        assert_eq!(copied, "中文\nsecond");
        let line = if cfg!(target_os = "macos") {
            "PASTE 12\n"
        } else {
            "HOTKEY 12 1 25\n"
        };
        assert_eq!(response.line, line);
        assert_eq!(response.message, "GPIO6: PASTE 12");
    }

    #[test]
    fn unmapped_press_is_skipped_without_clipboard_access() {
        let response = reply(
            Press {
                event_id: 12,
                gpio: 7,
            },
            None,
            |_| panic!("clipboard must not be called"),
        );

        assert_eq!(response.line, "SKIP 12\n");
        assert_eq!(response.message, "GPIO7: SKIP 12");
    }

    #[test]
    fn clipboard_failure_is_skipped() {
        let response = reply(
            Press {
                event_id: 12,
                gpio: 6,
            },
            Some(ButtonAction::Paste {
                text: "hello".into(),
            }),
            |_| Err("pbcopy exited 1".to_owned()),
        );

        assert_eq!(response.line, "SKIP 12\n");
        assert_eq!(
            response.message,
            "GPIO6: SKIP 12 (clipboard: pbcopy exited 1)"
        );
    }

    #[test]
    fn encodes_hid_hotkeys() {
        assert_eq!(
            encode_hotkey(&["cmd".into(), "shift".into(), "k".into()]),
            Ok((10, 14))
        );
        assert_eq!(encode_hotkey(&["page_down".into()]), Ok((0, 78)));
    }

    #[test]
    fn rejects_malformed_hotkeys() {
        for keys in [
            vec!["cmd", "cmd", "k"],
            vec!["cmd"],
            vec!["k", "l"],
            vec!["cmd", "unknown"],
        ] {
            assert!(
                encode_hotkey(&keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>())
                    .is_err(),
                "{keys:?} must be rejected"
            );
        }
    }

    #[test]
    fn hotkey_action_requests_hardware_shortcut() {
        let hotkey = Some(ButtonAction::Hotkey {
            keys: vec!["cmd".into(), "shift".into(), "k".into()],
        });

        assert_eq!(
            reply(
                Press {
                    event_id: 12,
                    gpio: 6
                },
                hotkey,
                |_| Ok(())
            )
            .line,
            "HOTKEY 12 10 14\n"
        );
    }
}
