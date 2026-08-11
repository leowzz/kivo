# Hotkey Label Internationalization Design

## Goal

Localize named keyboard keys throughout the Action editor. In Chinese, labels
must be short and direct, such as `方向左` and `右cmd`. English keeps the current
labels. Internal hotkey tokens, saved configuration, HID mapping, and firmware
protocols do not change.

This decision supersedes the key-label portion of section 2.5 in
`2026-08-10-action-device-workspace-followups.md`. Its remaining requirement
still applies: left and right physical keys must stay distinguishable, and every
consumer must show the same label for the same token.

## Scope

The language-aware label is used by:

- key checkboxes in every Hotkey Picker category;
- compact and physical modifier checkboxes;
- selected-key chips and their remove-button accessible names;
- recorded shortcut results;
- Action summaries;
- localized key search.

Letters, top-row digits, `F1` through `F24`, and symbol glyphs remain unchanged
in both languages. Numeric-keypad digits use the localized labels defined
below. Unknown tokens retain the existing uppercase fallback. Category and
validation-message localization continues to use the existing message catalog.

## Chinese Label Contract

| Group | Tokens and labels |
| --- | --- |
| Generic modifiers | `primary` -> `cmd/ctrl`; `cmd` -> `cmd`; `ctrl` -> `ctrl`; `alt`/`option` -> `option/alt`; `shift` -> `shift` |
| Physical modifiers | `left_cmd` -> `左cmd`; `right_cmd` -> `右cmd`; `left_ctrl` -> `左ctrl`; `right_ctrl` -> `右ctrl`; `left_alt` -> `左option/alt`; `right_alt` -> `右option/alt`; `left_shift` -> `左shift`; `right_shift` -> `右shift` |
| Common keys | `enter` -> `回车`; `escape` -> `Esc`; `backspace` -> `退格`; `tab` -> `Tab`; `space` -> `空格`; `caps_lock` -> `大写锁定` |
| Direction keys | `up` -> `方向上`; `down` -> `方向下`; `left` -> `方向左`; `right` -> `方向右` |
| Navigation keys | `insert` -> `插入`; `delete` -> `删除`; `home` -> `行首`; `end` -> `行尾`; `page_up`/`pageup` -> `上翻页`; `page_down`/`pagedown` -> `下翻页`; `print_screen` -> `截屏`; `scroll_lock` -> `滚动锁定`; `pause` -> `暂停`; `application` -> `菜单` |
| Numeric keypad | `num_lock` -> `数字锁定`; `numpad_0` through `numpad_9` -> `小键盘0` through `小键盘9`; operators keep their glyph after `小键盘`; `numpad_enter` -> `小键盘回车` |

## Approaches Considered

### Selected: one language-aware formatter backed by i18n

Keep a single token-to-message-key map beside the hotkey model. Change
`hotkeyDisplayLabel` to accept the active language and make `formatHotkey` use
the same function. The Picker and Action editor already receive `language`, so
they pass it through without changing stored data.

This preserves the existing single-source-of-truth fix while moving display
text into the repository's typed message catalog.

### Rejected: language maps inside `hotkey.ts`

This is mechanically small but creates a second localization system and loses
the message catalog's compile-time completeness checks.

### Rejected: translate only in `HotkeyPicker`

This repeats the earlier inconsistency: the picker and chips would be Chinese
while Action summaries remained English.

## Data Flow

The canonical token remains the source of selection identity, validation, and
serialization. Rendering passes `(language, token)` to the formatter. Action
summary formatting passes the same language through `formatHotkey`. Search
matches the localized display label, so Chinese users can search for `方向左`,
`回车`, or `右cmd`.

Changing language re-renders labels only. It must not call `onChange`, reorder
keys, or rewrite action data.

## Testing

Use a red-green cycle for these contracts:

1. Unit tests prove representative English labels are unchanged and Chinese
   labels include `方向左`, `右cmd`, `回车`, and `小键盘回车`.
2. Picker tests prove Chinese labels appear in modifier, navigation, selected
   chip, remove-button, and localized-search surfaces.
3. Action editor tests prove summaries use the active language while their
   underlying token arrays remain unchanged.
4. Existing validation, recording, category, and serialization tests remain
   green.

## Acceptance Criteria

- Chinese Action editing never shows `Arrow Left`, `Right Command`, or other
  English named-key labels covered by this spec.
- English labels remain compatible with the current UI.
- Picker, chip, recording, and Action summary labels agree for every token.
- Left/right physical modifier identity remains explicit.
- No hotkey token or persisted schema changes.
