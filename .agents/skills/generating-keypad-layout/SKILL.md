---
name: generating-keypad-layout
description: Use when creating or updating a Kivo Device Profile YAML keypad layout from a reference image in this repository.
---

# Generate Keypad Layout

Turn a device image into the normalized `profile` layout inside `models/prod/*.yaml`. Do not infer GPIO mappings or actions from the image.

## Required Inputs

Require both:

- A readable image path.
- An explicit device model ID.

If the model ID is missing, ask the user for it and stop. Do not infer it from the image, filename, existing models, or product appearance. If the image is missing or unreadable, ask for a usable image and stop.

## Workflow

1. Inspect the image with the available local image-viewing tool.
2. Read `src-tauri/src/model.rs`, `src-tauri/src/profile.rs`, and existing `models/prod/*.yaml`; current repository types and validation rules are authoritative.
3. Identify physical button regions and transcribe buttons in row-major visual order.
4. Ask before writing when the image leaves the button count, grouping, label, or semantic ID ambiguous.
5. Write or update `models/prod/<model-id>.yaml`. For a new file, use an empty `hardware_profiles` list and `actions` map. If it exists, ask before replacing it and preserve its hardware profiles and actions.
6. Run:

```bash
rtk yq '.' models/prod/<model-id>.yaml
rtk cargo test --manifest-path src-tauri/Cargo.toml model::tests
rtk git diff --check
```

Report the file path plus group and button counts.

## Layout Contract

| Field | Rule |
|---|---|
| `id` | Exact user-provided model ID; ASCII letters, digits, `-`, or `_` |
| `name` | User-provided display name, or a readable form of the model ID |
| `groups` | Ordered top-to-bottom physical regions |
| `group.id` | Short unique lowercase semantic name |
| `columns` | Visible column count for that group; at least 1 |
| `buttons` | Non-empty, ordered left-to-right then top-to-bottom |
| `button.id` | Globally unique stable semantic ID, normally uppercase `A-Z0-9_` |
| `button.label` | Visible text from the device, preserving symbols and combined labels |

Use equal-sized buttons within each group. Do not add pixel coordinates, rotation, per-button dimensions, GPIO values, actions, or image metadata. One physical key is one button: for example, `BACK/OUT` becomes `{ "id": "BACK_OUT", "label": "BACK/OUT" }`, not two buttons.

## Example

```yaml
schema_version: 2
profile:
  id: desk-phone-v2
  name: Desk Phone v2
  groups:
    - id: digits
      columns: 3
      buttons:
        - { id: DIGIT_1, label: "1" }
        - { id: DIGIT_2, label: "2" }
        - { id: DIGIT_3, label: "3" }
hardware_profiles: []
actions: {}
```

## Common Mistakes

- Guessing a missing model ID instead of asking.
- Splitting one combined physical key into multiple buttons.
- Encoding photograph geometry that the grouped layout cannot represent.
- Reusing a button ID within or across groups.
- Replacing existing hardware profiles or actions while updating only the layout.
- Modifying `config.yaml` while generating a model layout.
