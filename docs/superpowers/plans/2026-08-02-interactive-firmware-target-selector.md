# Interactive Firmware Target Selector Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Let each named firmware upload target open a live, explicitly confirmed device picker when `SERIAL` is absent.

**Architecture:** Reuse the existing USB inventory as the observation source, add a pure tracker for first-seen and selection state, and put a `prompt-toolkit` full-screen UI around it. Keep stdout machine-readable so Make can capture the selected serial, and resolve the serial before invoking any build or upload command.

**Tech Stack:** Python 3.13, prompt-toolkit 3, pytest, GNU Make, uv

---

### Task 1: Deterministic Device Tracking

**Files:**
- Create: `scripts/select_firmware_target.py`
- Create: `test/test_firmware_target_selector.py`

- [x] **Step 1: Write failing tracker tests**

Define tuple fixtures matching `list_firmware_targets.py` rows and assert that
`TargetTracker(board, allowed_modes).update(rows, now)`:

```python
tracker = TargetTracker("vccgnd-yd-rp2040", {"runtime", "bootloader"})
tracker.update([rp_row("SERIAL-A", "/dev/a")], datetime(2026, 8, 2, 12, 0, 0))
assert tracker.rows[0].connected_at is None

tracker.update(
    [rp_row("SERIAL-A", "/dev/a"), rp_row("SERIAL-B", "/dev/b")],
    datetime(2026, 8, 2, 12, 0, 3),
)
assert tracker.rows[1].connected_at == datetime(2026, 8, 2, 12, 0, 3)
```

Add separate tests for removal and reappearance, anonymous disabled rows,
disallowed modes, duplicate serials on distinct ports, movement over selectable
rows, and retaining the selected observation across refreshes.

- [x] **Step 2: Run tests and verify RED**

Run: `uv run pytest test/test_firmware_target_selector.py -q`

Expected: collection fails because `scripts.select_firmware_target` does not
exist.

- [x] **Step 3: Implement the pure tracker**

Add immutable `TargetRow` values and a `TargetTracker` with this public shape:

```python
InventoryRow = tuple[str, tuple[int, int], str, str | None, str | None]
TargetKey = tuple[str, tuple[int, int], str, str | None, str | None]

@dataclass(frozen=True)
class TargetRow:
    key: TargetKey
    mode: str
    usb_id: tuple[int, int]
    board: str
    serial_number: str | None
    port: str | None
    connected_at: datetime | None
    disabled_reason: str | None

    @property
    def selectable(self) -> bool:
        return self.disabled_reason is None

class TargetTracker:
    def __init__(self, board: str, allowed_modes: set[str]) -> None:
        self.board = board
        self.allowed_modes = frozenset(allowed_modes)
        self.rows: list[TargetRow] = []
        self.selected_key: TargetKey | None = None
        self._active_seen: dict[TargetKey, datetime | None] = {}
        self._has_snapshot = False

    def update(self, observations: Iterable[InventoryRow], now: datetime) -> None:
        candidates = [row for row in observations if row[2] == self.board]
        serial_counts = Counter(row[3] for row in candidates if row[3])
        active_seen: dict[TargetKey, datetime | None] = {}
        rows = []
        for mode, usb_id, board, serial_number, port in candidates:
            key = (mode, usb_id, board, serial_number, port)
            connected_at = self._active_seen.get(key)
            if key not in self._active_seen and self._has_snapshot:
                connected_at = now
            active_seen[key] = connected_at
            if not serial_number:
                disabled_reason = "missing hardware serial"
            elif mode not in self.allowed_modes:
                disabled_reason = f"{mode} mode cannot use this upload flow"
            elif serial_counts[serial_number] > 1:
                disabled_reason = "duplicate hardware serial"
            else:
                disabled_reason = None
            rows.append(TargetRow(key, mode, usb_id, board, serial_number, port, connected_at, disabled_reason))
        self.rows = rows
        self._active_seen = active_seen
        self._has_snapshot = True
        selectable = [row.key for row in rows if row.selectable]
        if self.selected_key not in selectable:
            self.selected_key = selectable[0] if selectable else None

    def move(self, delta: int) -> None:
        selectable = [row.key for row in self.rows if row.selectable]
        if not selectable:
            self.selected_key = None
            return
        if self.selected_key not in selectable:
            self.selected_key = selectable[0]
            return
        index = selectable.index(self.selected_key)
        self.selected_key = selectable[(index + delta) % len(selectable)]

    def selected(self) -> TargetRow | None:
        return next((row for row in self.rows if row.key == self.selected_key and row.selectable), None)
```

Treat the first snapshot as pre-existing, assign `now` only to keys first seen
after it, discard departed keys so reappearance gets a new timestamp, and
disable every current row whose Board Profile and serial are duplicated.

- [x] **Step 4: Run tracker tests and verify GREEN**

Run: `uv run pytest test/test_firmware_target_selector.py -q`

Expected: tracker tests pass.

### Task 2: Full-Screen Picker and CLI Contract

**Files:**
- Modify: `scripts/select_firmware_target.py`
- Modify: `test/test_firmware_target_selector.py`
- Modify: `pyproject.toml`
- Modify: `uv.lock`

- [x] **Step 1: Write failing presentation and CLI tests**

Test `format_target_rows(tracker)` without terminal escape sequences. Assert it
contains `connected before picker started`, later `HH:MM:SS` timestamps,
mode/serial/port fields, a visible disabled reason, and empty/error states.

Test `main()` through injected `stdin`, `stderr`, inventory, clock, and picker
runner boundaries. A non-TTY invocation must return 2 and instruct the operator
to pass `SERIAL=...`; selection must print only the serial to stdout;
cancellation must return nonzero and print no serial.

- [x] **Step 2: Run tests and verify RED**

Run: `uv run pytest test/test_firmware_target_selector.py -q`

Expected: failures name the missing formatting, picker, and CLI behavior.

- [x] **Step 3: Add prompt-toolkit**

Run: `uv add --dev 'prompt-toolkit>=3.0,<4'`

Expected: `pyproject.toml` and `uv.lock` record prompt-toolkit 3.x.

- [x] **Step 4: Implement the picker**

Parse repeatable `--mode` and required `--board`. Build a full-screen
`prompt_toolkit.application.Application` whose formatted-text body reads the
tracker. Bind Up/Down and `j`/`k` to movement, Enter to return a selectable
serial, `r` to set an immediate-refresh event, and `q`/Escape to exit with no
selection.

Use an async refresh loop that launches the existing inventory script as a
cancellable child process, a one-second timeout between scans, and
`create_output(stdout=sys.stderr)` so the terminal occupies stderr while stdout
remains clean. Terminate an in-progress inventory child when the picker exits,
catch inventory failures into visible state, and retry. Restore the terminal
through prompt-toolkit's normal application shutdown.

- [x] **Step 5: Run selector tests and verify GREEN**

Run: `uv run pytest test/test_firmware_target_selector.py -q`

Expected: all selector tests pass without opening a real terminal.

### Task 3: Make Upload Integration

**Files:**
- Modify: `Makefile`
- Modify: `test/test_release.sh`
- Create: `test/test_make_upload_selection.py`

- [x] **Step 1: Write failing Makefile contract tests**

Replace the old assertion that both uploads depend directly on
`require-serial`. Assert instead that each recipe:

```bash
grep -Fq 'serial="$(SERIAL)"' <<<"$body"
grep -Fq 'scripts/select_firmware_target.py' <<<"$body"
grep -Fq 'test -n "$$serial"' <<<"$body"
```

Also execute both upload recipes with an injected fake `UV` command. Assert an
explicit serial bypasses selection, selector failure stops before the build,
and a selected serial reaches build, upload, and verification in order without
touching hardware. Assert RP2040 passes runtime and bootloader modes, ESP32-S3
passes runtime only, and add both new pytest files to the ordered `make test`
command list.

- [x] **Step 2: Run the contract test and verify RED**

Run: `bash test/test_release.sh`

Expected: failure because upload targets still require `SERIAL` and never call
the selector.

- [x] **Step 3: Integrate serial resolution before builds**

Factor the two existing build commands into `ESP32S3_BUILD` and `RP2040_BUILD`
variables so public build targets and upload recipes share them. Use these
complete recipes:

```make
upload-esp32s3:
	@set -e; \
	  serial="$(SERIAL)"; \
	  if [ -z "$$serial" ]; then \
	    serial="$$(uv run python scripts/select_firmware_target.py --board luatos-esp32s3-aio --mode runtime)"; \
	  fi; \
	  test -n "$$serial" || { echo "SERIAL is required" >&2; exit 2; }; \
	  $(ESP32S3_BUILD); \
	  download_port="$$(uv run python scripts/enter_download_mode.py --serial "$$serial")"; \
	  KIVO_FIRMWARE_BUILD_ID="$(BUILD_ID)" uv run pio run -e esp32s3 -t upload --upload-port "$$download_port"; \
	  uv run pio pkg exec -p tool-esptoolpy -- esptool.py --chip esp32s3 --port "$$download_port" --after hard_reset run; \
	  uv run python scripts/verify_runtime_firmware.py --serial "$$serial" --vid 0x303a --pid 0x4002 --family esp32s3 --board luatos-esp32s3-aio --build "$(BUILD_ID)"

upload-rp2040:
	@set -e; \
	  serial="$(SERIAL)"; \
	  if [ -z "$$serial" ]; then \
	    serial="$$(uv run python scripts/select_firmware_target.py --board vccgnd-yd-rp2040 --mode runtime --mode bootloader)"; \
	  fi; \
	  test -n "$$serial" || { echo "SERIAL is required" >&2; exit 2; }; \
	  $(RP2040_BUILD); \
	  uv run pio pkg exec -p tool-picotool-rp2040-earlephilhower -- picotool load -x .pio/build/rp2040/firmware.uf2 --ser "$$serial"; \
	  uv run python scripts/verify_runtime_firmware.py --serial "$$serial" --vid 0x2e8a --pid 0x102e --family rp2040 --board vccgnd-yd-rp2040 --build "$(BUILD_ID)"
```

Keep `require-serial` for the standalone `download-mode` target. Use
`vccgnd-yd-rp2040` with runtime and bootloader modes for RP2040, and
`luatos-esp32s3-aio` with runtime mode for ESP32-S3. Do not run the build when
selection is cancelled or unavailable.

- [x] **Step 4: Run focused tests and verify GREEN**

Run: `bash test/test_release.sh && uv run pytest test/test_upload_targeting.py test/test_firmware_target_selector.py test/test_make_upload_selection.py -q`

Expected: shell contracts and all Python upload-selection tests pass.

### Task 4: Final Verification

**Files:**
- Review: `Makefile`
- Review: `scripts/select_firmware_target.py`
- Review: `test/test_firmware_target_selector.py`
- Review: `test/test_make_upload_selection.py`
- Review: `pyproject.toml`
- Review: `uv.lock`

- [x] **Step 1: Run Python and release checks**

Run: `bash test/test_release.sh && uv run pytest test/test_upload_targeting.py test/test_firmware_target_selector.py test/test_make_upload_selection.py -q`

Expected: exit 0 with all tests passing.

- [x] **Step 2: Run the repository test target**

Run: `make test`

Expected: release checks, Python tests, native firmware tests, Rust tests and
clippy, frontend tests, and frontend build all exit 0.

- [x] **Step 3: Inspect final changes**

Run: `git diff --check && git status --short && git diff --stat`

Expected: no whitespace errors; only the selector feature files plus the
pre-existing user-owned `src/platform/rp2040.cpp` change are present.
