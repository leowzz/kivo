# Kivo Icon Design

## Goal

Replace Kivo's legacy icon with a recognizable programmable-keypad mark. Do not include telephony imagery or text.

## Visual

- Graphite rounded-square background.
- One raised, electric-blue keycap centered in the icon.
- A short white horizontal function mark on the keycap.
- Clean, high-contrast rendering that remains recognizable at 16 px.

## Delivery

- Generate one 1024 x 1024 PNG with `gpt-image-2`.
- Derive the existing PNG, ICNS, ICO, and tray-icon assets from that source while retaining their current file names and dimensions.
- Do not change Tauri configuration or application code.

## Verification

- Inspect the generated source and representative 16 px and 128 px exports.
- Confirm each icon file has its expected format and dimensions.
- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
