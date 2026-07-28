# Physical Keypad Press Feedback Design

## Goal

Show the real state of each mapped physical telephone key in the GUI. A key
stays visually pressed for as long as the physical input is held and returns
to normal after release.

This change exposes debounced raw input edges only. Long-press and double-click
recognition are intentionally left to a later consumer of the event stream.

## Firmware Input Events

Reuse `GpioTriggerController` as the single debounce boundary. Its existing
30 ms stable-input threshold applies to both falling and rising edges. Extend
its output from press-only events to input-state events:

```text
STATE <event_id> <gpio> DOWN
STATE <event_id> <gpio> UP
```

The event ID increases for every emitted stable edge. Input bounce that does
not remain stable for 30 ms emits nothing.

The existing action response protocol remains unchanged:

```text
PASTE <event_id>
HOTKEY <event_id> <modifier_mask> <keycode>
SKIP <event_id>
```

Only `DOWN` starts the existing action request/response flow. `UP` is a state
notification and does not require a response. This preserves one action per
physical press while providing enough raw state to derive gestures later.

## Desktop Event Flow

The Tauri worker parses both `DOWN` and `UP` state lines. For `DOWN`, it resolves
and executes the configured action exactly as it does today. For `UP`, it skips
action resolution. Both edges emit a frontend runtime event containing the GPIO
and its state.

The serialized runtime payload adds:

```text
pressed: true | false | null
```

Input `DOWN` maps to `true`, input `UP` maps to `false`, and connection or error
events without a GPIO use `null`.

## GUI State And Appearance

`App` maintains the set of currently pressed GPIOs from runtime events and
passes the mapped button IDs to `Keypad`. A mapped key receives an
`is-physically-pressed` class while its GPIO is down.

The pressed style uses a dark green background, white text, a slight downward
translation, and an inset shadow. It remains distinct from the existing
configuration selection state. Releasing the GPIO removes the class
immediately.

The pressed set is cleared when the device disconnects or the active model
changes. Unmapped GPIO events remain visible in the activity log but do not
highlight a GUI key.

## Compatibility

The firmware and desktop helper must be updated together because `PRESS` input
lines are replaced by `STATE` lines. Saved model layouts, IO maps, button
actions, and helper responses do not change.

## Verification

- Native firmware test: bounce emits nothing; stable `DOWN` and `UP` each emit
  one ordered state event.
- Rust protocol test: complete `STATE` lines parse and malformed state lines are
  rejected.
- Rust runtime serialization test: `pressed` serializes as true, false, or null.
- React test: a mapped runtime `DOWN` adds the pressed class, a matching `UP`
  removes it, and disconnect clears it.
- Run the existing native, Rust, frontend test, and build commands to catch
  protocol and type regressions.
