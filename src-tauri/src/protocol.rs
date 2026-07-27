use crate::config::MappingConfig;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Press {
    pub event_id: u64,
    pub gpio: u8,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Reply {
    pub line: String,
    pub message: String,
}

pub fn parse_press(line: &str) -> Option<Press> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "PRESS" {
        return None;
    }
    let event_id = parts.next()?.parse().ok()?;
    let gpio = parts.next()?.parse().ok()?;
    (parts.next().is_none()).then_some(Press { event_id, gpio })
}

pub fn reply(
    press: Press,
    mappings: &MappingConfig,
    copy: impl FnOnce(&str) -> Result<(), String>,
) -> Reply {
    if let Some(text) = mappings
        .buttons
        .get(&press.gpio)
        .filter(|text| !text.is_empty())
    {
        return match copy(text) {
            Ok(()) => Reply {
                line: format!("PASTE {}\n", press.event_id),
                message: format!("GPIO{}: PASTE {}", press.gpio, press.event_id),
            },
            Err(error) => Reply {
                line: format!("SKIP {}\n", press.event_id),
                message: format!(
                    "GPIO{}: SKIP {} (clipboard: {error})",
                    press.gpio, press.event_id
                ),
            },
        };
    }
    Reply {
        line: format!("SKIP {}\n", press.event_id),
        message: format!("GPIO{}: SKIP {}", press.gpio, press.event_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn mappings(values: &[(u8, &str)]) -> MappingConfig {
        MappingConfig::from_buttons(
            values
                .iter()
                .map(|(gpio, text)| (*gpio, (*text).to_owned()))
                .collect::<BTreeMap<_, _>>(),
        )
        .unwrap()
    }

    #[test]
    fn parses_only_complete_press_lines() {
        assert_eq!(
            parse_press("PRESS 12 6\n"),
            Some(Press {
                event_id: 12,
                gpio: 6
            })
        );
        assert_eq!(parse_press("PRESS nope 6\n"), None);
        assert_eq!(parse_press("OTHER 12 6\n"), None);
        assert_eq!(parse_press("PRESS 12 6 extra\n"), None);
    }

    #[test]
    fn mapped_press_copies_then_requests_paste() {
        let mut copied = String::new();

        let response = reply(
            Press {
                event_id: 12,
                gpio: 6,
            },
            &mappings(&[(6, "中文\nsecond")]),
            |text| {
                copied = text.to_owned();
                Ok(())
            },
        );

        assert_eq!(copied, "中文\nsecond");
        assert_eq!(response.line, "PASTE 12\n");
        assert_eq!(response.message, "GPIO6: PASTE 12");
    }

    #[test]
    fn unmapped_press_is_skipped_without_clipboard_access() {
        let response = reply(
            Press {
                event_id: 12,
                gpio: 7,
            },
            &mappings(&[]),
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
            &mappings(&[(6, "hello")]),
            |_| Err("pbcopy exited 1".to_owned()),
        );

        assert_eq!(response.line, "SKIP 12\n");
        assert_eq!(
            response.message,
            "GPIO6: SKIP 12 (clipboard: pbcopy exited 1)"
        );
    }
}
