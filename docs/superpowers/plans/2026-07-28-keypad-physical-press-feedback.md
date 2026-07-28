# Physical Keypad Press Feedback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show each mapped GUI key as pressed for exactly as long as its debounced physical telephone input is down.

**Architecture:** The existing firmware debounce controller will emit both stable input edges through a new `STATE <event_id> <gpio> DOWN|UP` protocol. Tauri will execute configured actions only for `DOWN` while forwarding both states to React, where a set of pressed GPIOs drives a distinct keypad class.

**Tech Stack:** C++17/PlatformIO/Unity, Rust/Tauri/Serde, React 19/TypeScript/Vitest/Testing Library, CSS.

## Global Constraints

- Keep `GpioTriggerController::kDebounceMs` at 30 ms and use it as the only debounce boundary.
- Emit raw `DOWN` and `UP` edges only; do not add long-press or double-click recognition.
- Keep `PASTE`, `HOTKEY`, and `SKIP` helper responses unchanged.
- Keep saved model layouts, IO maps, and button actions unchanged.
- Do not add dependencies.

---

### Task 1: Debounced Firmware State Events

**Files:**
- Modify: `lib/gpio_trigger/src/GpioTriggerController.h`
- Modify: `lib/gpio_trigger/src/GpioTriggerController.cpp`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.h`
- Modify: `lib/gpio_trigger/src/TriggerProtocol.cpp`
- Modify: `src/main.cpp`
- Test: `test/test_gpio_trigger/test_main.cpp`

**Interfaces:**
- Consumes: sampled `inputHigh` GPIO levels and the existing 30 ms debounce threshold.
- Produces: `InputEvent { id, gpio, state }`, `InputState::{Down, Up}`, and `formatInputEvent(const InputEvent&)` producing `STATE <id> <gpio> DOWN|UP\n`.
- Preserves: one pending action response per GPIO so simultaneous stable edges are not dropped.

- [ ] **Step 1: Write the failing double-edge debounce test**

Replace the current press-only debounce and serialization assertions with:

```cpp
void test_stable_edges_emit_once_after_debounce() {
  GpioTriggerController controller(0);

  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1000).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1010).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1020).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1049).has_value());

  const auto down = controller.updatePin(6, false, 1050);
  TEST_ASSERT_TRUE(down.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, down->id);
  TEST_ASSERT_EQUAL_UINT8(6, down->gpio);
  TEST_ASSERT_EQUAL(InputState::Down, down->state);
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1100).has_value());

  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1200).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, false, 1210).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1220).has_value());
  TEST_ASSERT_FALSE(controller.updatePin(6, true, 1249).has_value());

  const auto up = controller.updatePin(6, true, 1250);
  TEST_ASSERT_TRUE(up.has_value());
  TEST_ASSERT_EQUAL_UINT32(2, up->id);
  TEST_ASSERT_EQUAL_UINT8(6, up->gpio);
  TEST_ASSERT_EQUAL(InputState::Up, up->state);
}

void test_serializes_input_state_events() {
  TEST_ASSERT_EQUAL_STRING(
      "STATE 42 6 DOWN\n",
      formatInputEvent(InputEvent{42, 6, InputState::Down}).c_str());
  TEST_ASSERT_EQUAL_STRING(
      "STATE 43 6 UP\n",
      formatInputEvent(InputEvent{43, 6, InputState::Up}).c_str());
}

void test_tracks_pending_responses_per_gpio() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  const auto first = controller.updatePin(6, false, 30);
  controller.updatePin(7, false, 40);
  const auto second = controller.updatePin(7, false, 70);

  TEST_ASSERT_TRUE(first.has_value());
  TEST_ASSERT_TRUE(second.has_value());
  TEST_ASSERT_EQUAL_UINT32(1, first->id);
  TEST_ASSERT_EQUAL_UINT32(2, second->id);
  TEST_ASSERT_EQUAL(ResponseAction::Execute,
                    controller.handleResponse(second->id, true));
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  TEST_ASSERT_EQUAL(ResponseAction::Cleared,
                    controller.handleResponse(first->id, false));
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}

void test_pending_responses_expire() {
  GpioTriggerController controller(0);
  controller.updatePin(6, false, 0);
  TEST_ASSERT_TRUE(controller.updatePin(6, false, 30).has_value());

  controller.expire(2029);
  TEST_ASSERT_TRUE(controller.hasPendingEvent());
  controller.expire(2030);
  TEST_ASSERT_FALSE(controller.hasPendingEvent());
}
```

Replace `test_stable_low_emits_one_event_after_debounce`,
`test_pending_event_blocks_other_pins_until_timeout`, and
`test_serializes_press_event` plus their `main()` registrations with the four
tests above and these registrations:

```cpp
RUN_TEST(test_stable_edges_emit_once_after_debounce);
RUN_TEST(test_tracks_pending_responses_per_gpio);
RUN_TEST(test_pending_responses_expire);
RUN_TEST(test_serializes_input_state_events);
```

In `test_release_rearms_pin_for_a_later_press`, change the second down event ID
expectation because the intervening `UP` is event 2:

```cpp
TEST_ASSERT_EQUAL_UINT32(3, second->id);
```

- [ ] **Step 2: Run the native tests and verify RED**

Run: `rtk pio test -e native`

Expected: compilation fails because `InputState`, `InputEvent`, and
`formatInputEvent` do not exist.

- [ ] **Step 3: Implement the minimal state event model**

In `GpioTriggerController.h`, replace `PressEvent` and the return type with:

```cpp
enum class InputState { Down, Up };

struct InputEvent {
  std::uint32_t id;
  std::uint8_t gpio;
  InputState state;
};

std::optional<InputEvent> updatePin(std::uint8_t gpio, bool inputHigh,
                                    std::uint32_t nowMs);
```

Replace the global pending request fields with one fixed slot per supported
GPIO:

```cpp
struct PendingEvent {
  std::uint32_t id;
  std::uint32_t startedMs;
};

std::array<std::optional<PendingEvent>, kSupportedPins.size()> pendingEvents_{};
```

After the existing stable-edge check in `GpioTriggerController.cpp`, emit both
states and arm a response slot only for `DOWN`:

```cpp
state.stableHigh = state.rawHigh;
const InputState inputState = state.stableHigh ? InputState::Up : InputState::Down;
const InputEvent event{nextEventId_++, gpio, inputState};
if (inputState == InputState::Down) {
  pendingEvents_[*index] = PendingEvent{event.id, nowMs};
}
return event;
```

Update `handleResponse`, `expire`, and `hasPendingEvent` to read and reset
the fixed pending slots:

```cpp
const auto pending = std::find_if(
    pendingEvents_.begin(), pendingEvents_.end(),
    [eventId](const auto &entry) {
      return entry.has_value() && entry->id == eventId;
    });
if (pending == pendingEvents_.end()) {
  return ResponseAction::Ignored;
}
pending->reset();
return execute ? ResponseAction::Execute : ResponseAction::Cleared;

for (auto &entry : pendingEvents_) {
  if (entry.has_value() &&
      nowMs - entry->startedMs >= kResponseTimeoutMs) {
    entry.reset();
  }
}

return std::any_of(
    pendingEvents_.begin(), pendingEvents_.end(),
    [](const auto &entry) { return entry.has_value(); });
```

In `TriggerProtocol.h/.cpp`, replace `formatPressEvent` with:

```cpp
std::string formatInputEvent(const InputEvent &event) {
  return "STATE " + std::to_string(event.id) + " " +
         std::to_string(event.gpio) + " " +
         (event.state == InputState::Down ? "DOWN\n" : "UP\n");
}
```

In `src/main.cpp`, serialize the returned event with:

```cpp
const std::string message = formatInputEvent(*event);
```

- [ ] **Step 4: Run the native tests and verify GREEN**

Run: `rtk pio test -e native`

Expected: all native Unity tests pass.

- [ ] **Step 5: Commit the firmware protocol change**

```bash
rtk git add lib/gpio_trigger/src/GpioTriggerController.h lib/gpio_trigger/src/GpioTriggerController.cpp lib/gpio_trigger/src/TriggerProtocol.h lib/gpio_trigger/src/TriggerProtocol.cpp src/main.cpp test/test_gpio_trigger/test_main.cpp
rtk git commit -m "feat: emit debounced input state edges"
```

---

### Task 2: Tauri State Parsing And Forwarding

**Files:**
- Modify: `src-tauri/src/protocol.rs`
- Modify: `src-tauri/src/device.rs`

**Interfaces:**
- Consumes: `STATE <event_id> <gpio> DOWN|UP` firmware lines.
- Produces: `parse_input(&str) -> Option<InputEvent>` and frontend `RuntimeEvent { gpio, pressed }`, where `pressed` is `Some(true)` for down, `Some(false)` for up, and `None` for non-input events.

- [ ] **Step 1: Write failing parser and serialization tests**

Replace the press parser test in `src-tauri/src/protocol.rs` with:

```rust
#[test]
fn parses_only_complete_input_state_lines() {
    assert_eq!(
        parse_input("STATE 12 6 DOWN\n"),
        Some(InputEvent { event_id: 12, gpio: 6, state: InputState::Down })
    );
    assert_eq!(
        parse_input("STATE 13 6 UP\n"),
        Some(InputEvent { event_id: 13, gpio: 6, state: InputState::Up })
    );
    assert_eq!(parse_input("STATE nope 6 DOWN\n"), None);
    assert_eq!(parse_input("STATE 12 6 HELD\n"), None);
    assert_eq!(parse_input("STATE 12 6 DOWN extra\n"), None);
    assert_eq!(parse_input("PRESS 12 6\n"), None);
}
```

Replace the runtime serialization test in `src-tauri/src/device.rs` with:

```rust
#[test]
fn runtime_events_serialize_input_state_or_null() {
    let event = RuntimeEvent {
        timestamp_ms: 1,
        level: EventLevel::Info,
        message: "Waiting for device".into(),
        connection: ConnectionStatus::searching(),
        gpio: None,
        pressed: None,
    };
    let value = serde_json::to_value(&event).unwrap();
    assert!(value["gpio"].is_null());
    assert!(value["pressed"].is_null());

    let down = RuntimeEvent { gpio: Some(6), pressed: Some(true), ..event.clone() };
    assert_eq!(serde_json::to_value(down).unwrap()["pressed"], true);
    let up = RuntimeEvent { gpio: Some(6), pressed: Some(false), ..event };
    assert_eq!(serde_json::to_value(up).unwrap()["pressed"], false);
}
```

- [ ] **Step 2: Run Rust tests and verify RED**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: compilation fails because `parse_input`, `InputEvent`, `InputState`,
and `RuntimeEvent.pressed` do not exist.

- [ ] **Step 3: Parse state lines and forward both edges**

In `src-tauri/src/protocol.rs`, keep `Press` for the existing `reply` function
and replace `parse_press` with:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputState { Down, Up }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputEvent {
    pub event_id: u64,
    pub gpio: u8,
    pub state: InputState,
}

pub fn parse_input(line: &str) -> Option<InputEvent> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "STATE" { return None; }
    let event_id = parts.next()?.parse().ok()?;
    let gpio = parts.next()?.parse().ok()?;
    let state = match parts.next()? {
        "DOWN" => InputState::Down,
        "UP" => InputState::Up,
        _ => return None,
    };
    parts.next().is_none().then_some(InputEvent { event_id, gpio, state })
}
```

In `src-tauri/src/device.rs`, import `parse_input`, `InputState`, and `Press`.
Add the runtime field:

```rust
pub pressed: Option<bool>,
```

Replace the press-only worker branch with a state branch. `DOWN` retains the
current mapping, clipboard, response write, and error-level logic; `UP` emits
only the state event:

```rust
let Some(input) = parse_input(text) else { continue; };
let pressed = input.state == InputState::Down;
if pressed {
    let action = mappings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .resolved_action(input.gpio);
    let press = Press { event_id: input.event_id, gpio: input.gpio };
    let response = reply(
        press,
        action_for_press(&capture_next_gpio, action),
        copy_to_clipboard,
    );
    let level = if response.message.contains("(clipboard:") {
        EventLevel::Error
    } else {
        EventLevel::Info
    };
    emit(
        &app,
        &connection,
        level,
        response.message,
        Some(input.gpio),
        Some(true),
    );
    if let Err(error) = device
        .get_mut()
        .write_all(response.line.as_bytes())
        .and_then(|()| device.get_mut().flush())
    {
        emit(
            &app,
            &connection,
            EventLevel::Error,
            format!("Serial write failed: {error}"),
            None,
            None,
        );
        break;
    }
} else {
    emit(
        &app,
        &connection,
        EventLevel::Info,
        format!("GPIO{}: UP {}", input.gpio, input.event_id),
        Some(input.gpio),
        Some(false),
    );
}
```

Extend `emit` with `pressed: Option<bool>`, assign it to `RuntimeEvent`, and
pass `None` beside `gpio: None` at connection, scan, open, read, and disconnect
call sites.

- [ ] **Step 4: Run Rust tests and verify GREEN**

Run: `rtk cargo test --manifest-path src-tauri/Cargo.toml`

Expected: all Rust tests pass.

- [ ] **Step 5: Commit the desktop protocol change**

```bash
rtk git add src-tauri/src/protocol.rs src-tauri/src/device.rs
rtk git commit -m "feat: forward physical input state"
```

---

### Task 3: Persistent GUI Press Feedback

**Files:**
- Modify: `src/types.ts`
- Modify: `src/App.tsx`
- Modify: `src/Keypad.tsx`
- Modify: `src/App.css`
- Test: `src/App.test.tsx`

**Interfaces:**
- Consumes: frontend `RuntimeEvent.pressed: boolean | null` and the active model's existing GPIO-to-button map.
- Produces: `pressedButtonIds: ReadonlySet<string>` for `Keypad` and the `is-physically-pressed` CSS state.

- [ ] **Step 1: Write failing held-key and disconnect tests**

Add to `src/App.test.tsx`:

```tsx
test("shows a mapped key as pressed until its physical input is released", async () => {
  render(<App />);
  const key = await screen.findByRole("button", { name: "Configure 2" });

  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1,
    level: "info",
    message: "GPIO6: PASTE 1",
    connection: { state: "connected", port: "/dev/cu.test" },
    gpio: 6,
    pressed: true,
  } }));
  expect(key).toHaveClass("is-physically-pressed");

  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 2,
    level: "info",
    message: "GPIO6: UP 2",
    connection: { state: "connected", port: "/dev/cu.test" },
    gpio: 6,
    pressed: false,
  } }));
  expect(key).not.toHaveClass("is-physically-pressed");
});

test("clears physical press feedback when the device disconnects", async () => {
  render(<App />);
  const key = await screen.findByRole("button", { name: "Configure 2" });
  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 1,
    level: "info",
    message: "GPIO6: PASTE 1",
    connection: { state: "connected", port: "/dev/cu.test" },
    gpio: 6,
    pressed: true,
  } }));
  expect(key).toHaveClass("is-physically-pressed");

  act(() => onRuntimeEvent?.({ payload: {
    timestampMs: 2,
    level: "warning",
    message: "Waiting for device",
    connection: { state: "searching", port: null },
    gpio: null,
    pressed: null,
  } }));
  expect(key).not.toHaveClass("is-physically-pressed");
});
```

Add `pressed: true` to existing physical press payloads and `pressed: null` to
existing connection-only payloads so all fixtures satisfy the new contract.

- [ ] **Step 2: Run the focused frontend tests and verify RED**

Run: `rtk npm test -- src/App.test.tsx`

Expected: the new assertions fail because keypad keys never receive
`is-physically-pressed`.

- [ ] **Step 3: Track pressed GPIOs and render mapped pressed keys**

Add the required field in `src/types.ts`:

```ts
pressed: boolean | null;
```

In `App`, add state:

```tsx
const [pressedGpios, setPressedGpios] = useState<Set<number>>(() => new Set());
```

In the runtime listener, update the set before capture handling and restrict IO
capture to `DOWN` events:

```tsx
if (payload.connection.state !== "connected") {
  setPressedGpios(new Set());
} else if (payload.gpio !== null && payload.pressed !== null) {
  setPressedGpios((current) => {
    const next = new Set(current);
    if (payload.pressed) next.add(payload.gpio!);
    else next.delete(payload.gpio!);
    return next;
  });
}
if (capturingButtonRef.current && payload.gpio !== null && payload.pressed) {
  capturingButtonRef.current = null;
  setCapturedGpio(payload.gpio);
}
```

Clear the set whenever the active model changes, including selection, revert,
and snapshot recovery:

```tsx
useEffect(() => {
  setPressedGpios(new Set());
}, [activeModel]);
```

Derive mapped button IDs beside `activeLayout`:

```tsx
const pressedButtonIds = useMemo(() => new Set(
  Object.entries(ioMaps[activeModel] ?? {})
    .filter(([gpio]) => pressedGpios.has(Number(gpio)))
    .map(([, buttonId]) => buttonId),
), [activeModel, ioMaps, pressedGpios]);
```

Pass `pressedButtonIds` to `Keypad`. Add this prop in `src/Keypad.tsx`:

```ts
pressedButtonIds: ReadonlySet<string>;
```

Build the key classes without changing selection behavior:

```tsx
className={[
  "key",
  selectedButtonId === button.id && "is-selected",
  pressedButtonIds.has(button.id) && "is-physically-pressed",
].filter(Boolean).join(" ")}
```

Add the transition declaration to the existing `.key` rule:

```css
transition: background-color 80ms ease, color 80ms ease,
  transform 80ms ease, box-shadow 80ms ease;
```

Add the physical state after `.key.is-selected`:

```css

.key.is-physically-pressed {
  color: #fff;
  background: #246b53;
  border-color: #174d39;
  box-shadow: inset 0 2px 4px rgb(13 53 38 / 35%);
  transform: translateY(2px);
}
```

- [ ] **Step 4: Run focused and full frontend verification**

Run: `rtk npm test -- src/App.test.tsx`

Expected: all `App.test.tsx` tests pass.

Run: `rtk npm run build`

Expected: TypeScript and Vite build successfully with no errors.

- [ ] **Step 5: Commit the GUI feedback change**

```bash
rtk git add src/types.ts src/App.tsx src/Keypad.tsx src/App.css src/App.test.tsx
rtk git commit -m "feat: show physical keypad press state"
```

---

### Task 4: End-To-End Verification

**Files:**
- Verify only; no production files should change.

**Interfaces:**
- Consumes: completed firmware, Tauri, and React changes from Tasks 1-3.
- Produces: fresh evidence that the protocol, desktop runtime, GUI tests, and both builds agree.

- [ ] **Step 1: Run every automated check**

```bash
rtk pio test -e native
rtk pio run -e esp32s3
rtk cargo test --manifest-path src-tauri/Cargo.toml
rtk npm test
rtk npm run build
rtk git diff --check
```

Expected: every command exits 0, all tests pass, both firmware and desktop
builds succeed, and `git diff --check` prints no errors.

- [ ] **Step 2: Inspect the final scope**

Run: `rtk git status --short`

Expected: only the implementation-plan file may remain uncommitted; the
pre-existing `.agents/`, `models/prod/`, and `models/tel001.json` entries remain
untouched.
