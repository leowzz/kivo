import argparse
import asyncio
import csv
import sys
from collections import Counter
from collections.abc import Awaitable, Callable, Iterable, Sequence
from contextlib import suppress
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import TextIO

from prompt_toolkit.application import Application
from prompt_toolkit.input import Input
from prompt_toolkit.input.defaults import create_input
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.layout import Layout
from prompt_toolkit.layout.containers import Window
from prompt_toolkit.layout.controls import FormattedTextControl
from prompt_toolkit.output import Output
from prompt_toolkit.output.defaults import create_output


InventoryRow = tuple[str, tuple[int, int], str, str | None, str | None]
TargetKey = tuple[str, tuple[int, int], str, str | None, str | None]
Inventory = Callable[[], Awaitable[list[InventoryRow]]]
Clock = Callable[[], datetime]


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
        previous_index = next(
            (
                index
                for index, row in enumerate(self.rows)
                if row.key == self.selected_key
            ),
            None,
        )
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

            rows.append(
                TargetRow(
                    key=key,
                    mode=mode,
                    usb_id=usb_id,
                    board=board,
                    serial_number=serial_number,
                    port=port,
                    connected_at=connected_at,
                    disabled_reason=disabled_reason,
                )
            )

        self.rows = rows
        self._active_seen = active_seen
        self._has_snapshot = True
        selectable = [
            (index, row.key)
            for index, row in enumerate(rows)
            if row.selectable
        ]
        selectable_keys = [key for _, key in selectable]
        if self.selected_key not in selectable_keys:
            if not selectable:
                self.selected_key = None
            elif previous_index is None:
                self.selected_key = selectable[0][1]
            else:
                self.selected_key = min(
                    selectable,
                    key=lambda item: (abs(item[0] - previous_index), item[0]),
                )[1]

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
        return next(
            (
                row
                for row in self.rows
                if row.key == self.selected_key and row.selectable
            ),
            None,
        )


@dataclass
class PickerViewState:
    scanning: bool = True
    inventory_error: str | None = None


def format_target_rows(
    tracker: TargetTracker,
    *,
    scanning: bool = False,
    inventory_error: str | None = None,
) -> str:
    lines = [
        "Kivo firmware target selector",
        f"Board: {tracker.board}",
        "",
    ]

    if inventory_error:
        lines.extend([f"Scan error: {inventory_error}", "Retrying automatically..."])
    elif scanning:
        lines.append("Scanning for compatible USB devices...")
    elif not tracker.rows:
        lines.append("No matching devices detected. Plug one in or keep waiting.")
    else:
        for row in tracker.rows:
            marker = ">" if row.key == tracker.selected_key else " "
            connected = (
                row.connected_at.strftime("%H:%M:%S")
                if row.connected_at
                else "connected before picker started"
            )
            usb_id = f"{row.usb_id[0]:04x}:{row.usb_id[1]:04x}"
            lines.extend(
                [
                    f"{marker} {row.mode}  serial={row.serial_number or '-'}",
                    f"    USB={usb_id}  port={row.port or '-'}",
                    f"    connected={connected}",
                ]
            )
            if row.disabled_reason:
                lines.append(f"    unavailable: {row.disabled_reason}")
            lines.append("")

    lines.extend(
        [
            "",
            "Up/Down or j/k: select    Enter: confirm    r: refresh    q/Esc: cancel",
        ]
    )
    return "\n".join(lines)


def parse_inventory_rows(output: str) -> list[InventoryRow]:
    rows: list[InventoryRow] = []
    for line_number, fields in enumerate(
        csv.reader(output.splitlines(), delimiter="\t"),
        start=1,
    ):
        if len(fields) != 5:
            raise ValueError(
                f"inventory row {line_number} has {len(fields)} fields; expected 5"
            )
        mode, usb_id, board, serial_number, port = fields
        try:
            vid_text, pid_text = usb_id.split(":", maxsplit=1)
            parsed_usb_id = (int(vid_text, 16), int(pid_text, 16))
        except (TypeError, ValueError) as error:
            raise ValueError(
                f"inventory row {line_number} has invalid USB ID {usb_id!r}"
            ) from error
        rows.append(
            (
                mode,
                parsed_usb_id,
                board,
                None if serial_number == "-" else serial_number,
                None if port == "-" else port,
            )
        )
    return rows


async def scan_inventory() -> list[InventoryRow]:
    inventory_script = Path(__file__).with_name("list_firmware_targets.py")
    process = await asyncio.create_subprocess_exec(
        sys.executable,
        str(inventory_script),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        stdout, stderr = await process.communicate()
    except asyncio.CancelledError:
        if process.returncode is None:
            with suppress(ProcessLookupError):
                process.terminate()
            try:
                await asyncio.wait_for(process.wait(), timeout=0.5)
            except TimeoutError:
                with suppress(ProcessLookupError):
                    process.kill()
                await process.wait()
        raise

    if process.returncode:
        detail = stderr.decode(errors="replace").strip()
        raise RuntimeError(detail or f"inventory exited with {process.returncode}")
    return parse_inventory_rows(stdout.decode(errors="strict"))


def confirmation_serial(
    tracker: TargetTracker,
    *,
    inventory_error: str | None,
) -> str | None:
    if inventory_error:
        return None
    selected = tracker.selected()
    return selected.serial_number if selected else None


async def run_picker_async(
    *,
    tracker: TargetTracker,
    inventory: Inventory,
    clock: Clock,
    stdin: TextIO,
    stderr: TextIO,
    refresh_interval: float = 1.0,
    prompt_input: Input | None = None,
    prompt_output: Output | None = None,
) -> str | None:
    view_state = PickerViewState()
    refresh_requested = asyncio.Event()
    bindings = KeyBindings()

    def render() -> str:
        return format_target_rows(
            tracker,
            scanning=view_state.scanning,
            inventory_error=view_state.inventory_error,
        )

    @bindings.add("up")
    @bindings.add("k")
    def move_up(event: object) -> None:
        tracker.move(-1)
        event.app.invalidate()

    @bindings.add("down")
    @bindings.add("j")
    def move_down(event: object) -> None:
        tracker.move(1)
        event.app.invalidate()

    @bindings.add("enter")
    def confirm(event: object) -> None:
        selected_serial = confirmation_serial(
            tracker,
            inventory_error=view_state.inventory_error,
        )
        if selected_serial:
            event.app.exit(result=selected_serial)

    @bindings.add("r")
    def refresh(event: object) -> None:
        refresh_requested.set()
        event.app.invalidate()

    @bindings.add("q")
    @bindings.add("escape")
    @bindings.add("c-c")
    def cancel(event: object) -> None:
        event.app.exit(result=None)

    owns_input = prompt_input is None
    application_input = create_input(stdin=stdin) if prompt_input is None else prompt_input
    application_output = (
        create_output(stdout=stderr) if prompt_output is None else prompt_output
    )
    application: Application[str | None] = Application(
        layout=Layout(
            Window(
                content=FormattedTextControl(render),
                wrap_lines=True,
                always_hide_cursor=True,
            )
        ),
        key_bindings=bindings,
        full_screen=True,
        input=application_input,
        output=application_output,
    )

    async def refresh_once() -> None:
        try:
            observations = await inventory()
        except Exception as error:
            view_state.inventory_error = str(error)
        else:
            tracker.update(observations, clock())
            view_state.inventory_error = None
        finally:
            view_state.scanning = False
            application.invalidate()

    async def refresh_loop() -> None:
        while True:
            await refresh_once()
            try:
                await asyncio.wait_for(
                    refresh_requested.wait(),
                    timeout=refresh_interval,
                )
            except TimeoutError:
                pass
            refresh_requested.clear()

    refresh_task = asyncio.create_task(refresh_loop())
    try:
        return await application.run_async()
    finally:
        refresh_task.cancel()
        with suppress(asyncio.CancelledError):
            await refresh_task
        if owns_input:
            application_input.close()


def run_picker(
    *,
    tracker: TargetTracker,
    inventory: Inventory,
    clock: Clock,
    stdin: TextIO,
    stderr: TextIO,
) -> str | None:
    return asyncio.run(
        run_picker_async(
            tracker=tracker,
            inventory=inventory,
            clock=clock,
            stdin=stdin,
            stderr=stderr,
        )
    )


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Interactively select a connected firmware upload target."
    )
    parser.add_argument("--board", required=True)
    parser.add_argument(
        "--mode",
        action="append",
        choices=("runtime", "bootloader"),
        required=True,
        dest="modes",
    )
    return parser.parse_args(argv)


def main(
    argv: Sequence[str] | None = None,
    *,
    stdin: TextIO | None = None,
    stdout: TextIO | None = None,
    stderr: TextIO | None = None,
    picker: Callable[..., str | None] = run_picker,
) -> int:
    args = parse_args(argv)
    stdin = sys.stdin if stdin is None else stdin
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr

    if not stdin.isatty() or not stderr.isatty():
        print(
            "Interactive target selection requires a terminal; "
            "pass SERIAL=<hardware serial>.",
            file=stderr,
        )
        return 2

    tracker = TargetTracker(args.board, set(args.modes))
    try:
        selected_serial = picker(
            tracker=tracker,
            inventory=scan_inventory,
            clock=datetime.now,
            stdin=stdin,
            stderr=stderr,
        )
    except KeyboardInterrupt:
        selected_serial = None

    if selected_serial is None:
        print("Firmware upload cancelled.", file=stderr)
        return 130

    print(selected_serial, file=stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
