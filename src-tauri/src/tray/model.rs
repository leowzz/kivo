#![allow(dead_code)]

use crate::{
    model::ButtonDefinition,
    profile::ButtonAction,
    workspace::Language,
};

const PRIMARY_PASTE_LIMIT: usize = 12;
const DETAIL_PASTE_LIMIT: usize = 80;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TrayCopy {
    pub waiting: &'static str,
    pub empty: &'static str,
    pub unconfigured: &'static str,
    pub open: &'static str,
    pub quit: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TrayButton {
    pub title: String,
    pub details: Vec<String>,
}

#[derive(Clone, Copy)]
enum SummaryKind {
    Primary,
    Detail,
}

pub(super) fn copy(language: Language) -> TrayCopy {
    match language {
        Language::ZhCn => TrayCopy {
            waiting: "等待设备",
            empty: "暂无可用设备",
            unconfigured: "未配置",
            open: "打开 Kivo",
            quit: "退出 Kivo",
        },
        Language::EnUs => TrayCopy {
            waiting: "Waiting for device",
            empty: "No available device",
            unconfigured: "Not configured",
            open: "Open Kivo",
            quit: "Quit Kivo",
        },
    }
}

pub(super) fn button_summary(
    button: &ButtonDefinition,
    actions: &[ButtonAction],
    language: Language,
) -> TrayButton {
    let label = escape_menu_text(&collapse_whitespace(&button.label));
    let Some(first) = actions.first() else {
        return TrayButton {
            title: format!("{label} · {}", copy(language).unconfigured),
            details: Vec::new(),
        };
    };
    let remaining = actions.len() - 1;
    let suffix = if remaining == 0 {
        String::new()
    } else {
        format!(" +{remaining}")
    };
    TrayButton {
        title: format!("{label} · {}{suffix}", format_action(first, SummaryKind::Primary)),
        details: actions
            .iter()
            .map(|action| format_action(action, SummaryKind::Detail))
            .collect(),
    }
}

fn format_action(action: &ButtonAction, kind: SummaryKind) -> String {
    match action {
        ButtonAction::Paste { text } => {
            let (cleaned, limit) = match kind {
                SummaryKind::Primary => (collapse_whitespace(text), PRIMARY_PASTE_LIMIT),
                SummaryKind::Detail => (visible_line_breaks(text), DETAIL_PASTE_LIMIT),
            };
            format!("“{}”", escape_menu_text(&truncate_chars(&cleaned, limit)))
        }
        ButtonAction::Hotkey { keys } => keys
            .iter()
            .map(|key| key_abbreviation(key))
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn key_abbreviation(key: &str) -> String {
    match key.to_ascii_lowercase().as_str() {
        "cmd" => "⌘".into(),
        "alt" | "option" => "⌥".into(),
        "ctrl" => "⌃".into(),
        "shift" => "⇧".into(),
        "enter" => "↩".into(),
        "tab" => "⇥".into(),
        "backspace" => "⌫".into(),
        "escape" => "Esc".into(),
        "delete" => "Del".into(),
        "space" => "Space".into(),
        "up" => "↑".into(),
        "down" => "↓".into(),
        "left" => "←".into(),
        "right" => "→".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        "page_up" | "pageup" => "PgUp".into(),
        "page_down" | "pagedown" => "PgDn".into(),
        other => other.to_uppercase(),
    }
}

fn collapse_whitespace(value: &str) -> String {
    let without_controls = value
        .chars()
        .filter(|character| !character.is_control() || character.is_whitespace())
        .collect::<String>();
    without_controls.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn visible_line_breaks(value: &str) -> String {
    let lines = value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', " ↵ ");
    collapse_whitespace(&lines)
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(limit).collect::<String>();
    if characters.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn escape_menu_text(value: &str) -> String {
    value.replace('&', "&&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::ButtonDefinition,
        profile::ButtonAction,
        workspace::Language,
    };

    #[test]
    fn formats_paste_preview_detail_and_remaining_count() {
        let button = ButtonDefinition {
            id: "CONFIRM".into(),
            label: "确认 & 发送".into(),
        };
        let actions = vec![
            ButtonAction::Paste {
                text: "  订单号\n1234567890  ".into(),
            },
            ButtonAction::Hotkey {
                keys: vec!["cmd".into(), "shift".into(), "k".into()],
            },
            ButtonAction::Paste {
                text: "A\r\nB\0C".into(),
            },
        ];

        let summary = button_summary(&button, &actions, Language::ZhCn);

        assert_eq!(summary.title, "确认 && 发送 · “订单号 12345678…” +2");
        assert_eq!(
            summary.details,
            vec!["“订单号 ↵ 1234567890”", "⌘⇧K", "“A ↵ BC”"],
        );
    }

    #[test]
    fn formats_macos_key_abbreviations_and_unconfigured_copy() {
        let button = ButtonDefinition {
            id: "KEY".into(),
            label: "Key".into(),
        };
        let keys = [
            (vec!["cmd", "c"], "⌘C"),
            (vec!["ctrl", "alt", "shift", "enter"], "⌃⌥⇧↩"),
            (vec!["option", "1"], "⌥1"),
            (vec!["tab"], "⇥"),
            (vec!["backspace"], "⌫"),
            (vec!["escape"], "Esc"),
            (vec!["delete"], "Del"),
            (vec!["space"], "Space"),
            (vec!["up", "down", "left", "right"], "↑↓←→"),
            (vec!["home", "end", "page_up", "page_down"], "HomeEndPgUpPgDn"),
        ];
        for (configured, expected) in keys {
            let action = ButtonAction::Hotkey {
                keys: configured.into_iter().map(str::to_owned).collect(),
            };
            assert_eq!(format_action(&action, SummaryKind::Primary), expected);
        }

        let long_paste = ButtonAction::Paste { text: "界".repeat(81) };
        assert_eq!(
            format_action(&long_paste, SummaryKind::Detail),
            format!("“{}…”", "界".repeat(80)),
        );

        assert_eq!(
            button_summary(&button, &[], Language::ZhCn).title,
            "Key · 未配置",
        );
        assert_eq!(
            button_summary(&button, &[], Language::EnUs).title,
            "Key · Not configured",
        );
    }

    #[test]
    fn localizes_all_static_tray_copy() {
        assert_eq!(
            copy(Language::ZhCn),
            TrayCopy {
                waiting: "等待设备",
                empty: "暂无可用设备",
                unconfigured: "未配置",
                open: "打开 Kivo",
                quit: "退出 Kivo",
            },
        );
        assert_eq!(
            copy(Language::EnUs),
            TrayCopy {
                waiting: "Waiting for device",
                empty: "No available device",
                unconfigured: "Not configured",
                open: "Open Kivo",
                quit: "Quit Kivo",
            },
        );
    }
}
