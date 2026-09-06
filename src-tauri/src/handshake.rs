use crate::{error::AppError, hardware::BoardProfile, input::HOST_PROTOCOL_VERSION};
use serde::Serialize;
use std::collections::BTreeSet;

const PRODUCT_DEFINITION_PROTOCOL_VERSION: u16 = 9;
const MIN_SUPPORTED_PROTOCOL_VERSION: u16 = 3;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloCapabilities {
    pub protocol: u16,
    pub controller_family_id: String,
    pub board_profile_id: String,
    pub firmware_build_id: String,
    pub product_version_id: Option<String>,
    pub pins: Vec<u8>,
}

pub fn validate_hello(
    candidate_board: &BoardProfile,
    hello: &HelloCapabilities,
) -> Result<(), AppError> {
    if !(MIN_SUPPORTED_PROTOCOL_VERSION..=HOST_PROTOCOL_VERSION).contains(&hello.protocol) {
        return Err(AppError::new("protocol_mismatch")
            .with_param("expected", HOST_PROTOCOL_VERSION.to_string())
            .with_param("actual", hello.protocol.to_string()));
    }
    if hello.protocol < PRODUCT_DEFINITION_PROTOCOL_VERSION && hello.product_version_id.is_some() {
        return Err(AppError::new("protocol_mismatch"));
    }
    if let Some(product_version_id) = &hello.product_version_id
        && !crate::product::valid_product_version_id(product_version_id)
    {
        return Err(AppError::new("invalid_product_version_id"));
    }
    if hello.controller_family_id != candidate_board.family_id {
        return Err(AppError::new("controller_family_mismatch")
            .with_param("expected", candidate_board.family_id)
            .with_param("actual", &hello.controller_family_id));
    }
    if !crate::hardware::board_profile_ids_match(&hello.board_profile_id, candidate_board.id) {
        return Err(AppError::new("board_profile_mismatch")
            .with_param("expected", candidate_board.id)
            .with_param("actual", &hello.board_profile_id));
    }
    if let Some(pin) = hello
        .pins
        .iter()
        .find(|pin| !candidate_board.safe_pins.contains(pin))
    {
        return Err(AppError::new("capability_mismatch").with_param("gpio", pin.to_string()));
    }
    Ok(())
}

pub(crate) fn is_hello_line(line: &str) -> bool {
    line.trim_start_matches(char::is_whitespace)
        .starts_with("HELLO")
}

pub(crate) fn parse_hello(line: &str) -> Option<HelloCapabilities> {
    if line.len() >= 255 {
        return None;
    }
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty()
        || line.starts_with(' ')
        || line.ends_with(' ')
        || line
            .chars()
            .any(|character| character.is_whitespace() && character != ' ')
    {
        return None;
    }
    let parts = line.split(' ').collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let ["HELLO", protocol, remainder @ ..] = parts.as_slice() else {
        return None;
    };
    let protocol = protocol.parse::<u16>().ok()?;
    if !(MIN_SUPPORTED_PROTOCOL_VERSION..=HOST_PROTOCOL_VERSION).contains(&protocol) {
        return None;
    }
    let (
        controller_family_id,
        board_profile_id,
        firmware_build_id,
        product_version_id,
        count,
        pins,
    ) = if protocol >= 9 {
        let [
            controller_family_id,
            board_profile_id,
            firmware_build_id,
            product_version_id,
            count,
            pins @ ..,
        ] = remainder
        else {
            return None;
        };
        let product_version_id =
            (*product_version_id != "-").then(|| (*product_version_id).to_owned());
        if product_version_id
            .as_deref()
            .is_some_and(|id| !crate::product::valid_product_version_id(id))
        {
            return None;
        }
        (
            *controller_family_id,
            *board_profile_id,
            *firmware_build_id,
            product_version_id,
            *count,
            pins,
        )
    } else {
        let [
            controller_family_id,
            board_profile_id,
            firmware_build_id,
            count,
            pins @ ..,
        ] = remainder
        else {
            return None;
        };
        (
            *controller_family_id,
            *board_profile_id,
            *firmware_build_id,
            None,
            *count,
            pins,
        )
    };
    let count = count.parse::<usize>().ok()?;
    let pins = pins
        .iter()
        .map(|pin| pin.parse::<u8>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    (count > 0
        && count == pins.len()
        && pins.iter().copied().collect::<BTreeSet<_>>().len() == count)
        .then(|| HelloCapabilities {
            protocol,
            controller_family_id: controller_family_id.to_owned(),
            board_profile_id: board_profile_id.to_owned(),
            firmware_build_id: firmware_build_id.to_owned(),
            product_version_id,
            pins,
        })
}
