use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonDefinition {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ButtonGroup {
    pub id: String,
    pub columns: usize,
    pub buttons: Vec<ButtonDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ModelLayout {
    pub id: String,
    pub name: String,
    pub groups: Vec<ButtonGroup>,
}

impl ModelLayout {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("model id and name are required".into());
        }
        if !self
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err("model id must use ASCII letters, digits, hyphens, or underscores".into());
        }
        let mut groups = BTreeSet::new();
        let mut buttons = BTreeSet::new();
        for group in &self.groups {
            if group.id.trim().is_empty() || !groups.insert(group.id.as_str()) {
                return Err(format!("invalid or duplicate group {}", group.id));
            }
            if group.columns == 0 || group.buttons.is_empty() {
                return Err(format!("group {} must have columns and buttons", group.id));
            }
            for button in &group.buttons {
                if button.id.trim().is_empty() || button.label.trim().is_empty() {
                    return Err("button id and label are required".into());
                }
                if !buttons.insert(button.id.as_str()) {
                    return Err(format!("duplicate button {}", button.id));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> ModelLayout {
        ModelLayout {
            id: "phone".into(),
            name: "Phone".into(),
            groups: vec![ButtonGroup {
                id: "keys".into(),
                columns: 1,
                buttons: vec![ButtonDefinition {
                    id: "A".into(),
                    label: "A".into(),
                }],
            }],
        }
    }

    #[test]
    fn validates_a_layout() {
        layout().validate().unwrap();
    }

    #[test]
    fn rejects_duplicate_button_ids() {
        let mut layout = layout();
        layout.groups.push(ButtonGroup {
            id: "other".into(),
            columns: 1,
            buttons: vec![ButtonDefinition {
                id: "A".into(),
                label: "Duplicate".into(),
            }],
        });
        assert_eq!(layout.validate().unwrap_err(), "duplicate button A");
    }
}
