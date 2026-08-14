import asyncio
from datetime import datetime
from io import StringIO

import scripts.select_firmware_target as target_selector
from prompt_toolkit.input import create_pipe_input
from prompt_toolkit.output import DummyOutput

from scripts.select_firmware_target import (
    TargetTracker,
    confirmation_serial,
    format_target_rows,
    main,
    parse_inventory_rows,
    run_picker_async,
)


RP_BOARD = "yd-rp2040"
RP_RUNTIME_USB = (0x2E8A, 0x102E)


def rp_row(
    serial_number: str | None,
    port: str | None,
    *,
    mode: str = "runtime",
) -> tuple[str, tuple[int, int], str, str | None, str | None]:
    return mode, RP_RUNTIME_USB, RP_BOARD, serial_number, port


def test_parses_inventory_tsv_without_treating_placeholders_as_values() -> None:
    output = (
        "runtime\t2e8a:102e\tyd-rp2040\tSERIAL-A\t/dev/a\n"
        "bootloader\t2e8a:0003\tyd-rp2040\t-\t-\n"
    )

    assert parse_inventory_rows(output) == [
        ("runtime", (0x2E8A, 0x102E), RP_BOARD, "SERIAL-A", "/dev/a"),
        ("bootloader", (0x2E8A, 0x0003), RP_BOARD, None, None),
    ]


def test_tracks_initial_and_later_connection_times() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime", "bootloader"})
    initial_time = datetime(2026, 8, 2, 12, 0, 0)
    later_time = datetime(2026, 8, 2, 12, 0, 3)

    tracker.update([rp_row("SERIAL-A", "/dev/a")], initial_time)
    assert tracker.rows[0].connected_at is None

    tracker.update(
        [rp_row("SERIAL-A", "/dev/a"), rp_row("SERIAL-B", "/dev/b")],
        later_time,
    )

    assert tracker.rows[0].connected_at is None
    assert tracker.rows[1].connected_at == later_time


def test_reappearing_observation_gets_a_new_connection_time() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    tracker.update([rp_row("SERIAL-A", "/dev/a")], datetime(2026, 8, 2, 12, 0, 0))
    tracker.update([], datetime(2026, 8, 2, 12, 0, 1))

    reconnected_at = datetime(2026, 8, 2, 12, 0, 2)
    tracker.update([rp_row("SERIAL-A", "/dev/a")], reconnected_at)

    assert tracker.rows[0].connected_at == reconnected_at


def test_disables_rows_without_serial_or_in_an_unsupported_mode() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})

    tracker.update(
        [
            rp_row(None, "/dev/no-serial"),
            rp_row("BOOT", "/dev/boot", mode="bootloader"),
            rp_row("READY", "/dev/ready"),
        ],
        datetime(2026, 8, 2, 12, 0, 0),
    )

    assert [row.disabled_reason for row in tracker.rows] == [
        "missing hardware serial",
        "bootloader mode cannot use this upload flow",
        None,
    ]
    assert tracker.selected().serial_number == "READY"


def test_disables_duplicate_serials_without_collapsing_rows() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})

    tracker.update(
        [rp_row("DUPLICATE", "/dev/a"), rp_row("DUPLICATE", "/dev/b")],
        datetime(2026, 8, 2, 12, 0, 0),
    )

    assert [row.port for row in tracker.rows] == ["/dev/a", "/dev/b"]
    assert [row.disabled_reason for row in tracker.rows] == [
        "duplicate hardware serial",
        "duplicate hardware serial",
    ]
    assert tracker.selected() is None


def test_selection_skips_disabled_rows_wraps_and_survives_refresh() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    rows = [
        rp_row(None, "/dev/no-serial"),
        rp_row("SERIAL-A", "/dev/a"),
        rp_row("SERIAL-B", "/dev/b"),
    ]
    tracker.update(rows, datetime(2026, 8, 2, 12, 0, 0))
    assert tracker.selected().serial_number == "SERIAL-A"

    tracker.move(1)
    assert tracker.selected().serial_number == "SERIAL-B"

    tracker.update(list(reversed(rows)), datetime(2026, 8, 2, 12, 0, 1))
    assert tracker.selected().serial_number == "SERIAL-B"

    tracker.move(1)
    assert tracker.selected().serial_number == "SERIAL-A"


def test_selection_moves_to_nearest_row_when_selected_device_disappears() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    rows = [
        rp_row("SERIAL-A", "/dev/a"),
        rp_row("SERIAL-B", "/dev/b"),
        rp_row("SERIAL-C", "/dev/c"),
    ]
    tracker.update(rows, datetime(2026, 8, 2, 12, 0, 0))
    tracker.move(-1)
    assert tracker.selected().serial_number == "SERIAL-C"

    tracker.update(rows[:2], datetime(2026, 8, 2, 12, 0, 1))

    assert tracker.selected().serial_number == "SERIAL-B"


def test_filters_out_other_board_profiles() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    other = ("runtime", (0x303A, 0x4002), "yd-esp32-s3", "ESP", "/dev/esp")

    tracker.update([other, rp_row("RP", "/dev/rp")], datetime(2026, 8, 2, 12, 0, 0))

    assert [row.serial_number for row in tracker.rows] == ["RP"]


def test_formats_target_details_connection_times_and_disabled_reasons() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    tracker.update(
        [rp_row("SERIAL-A", "/dev/a"), rp_row(None, "/dev/no-serial")],
        datetime(2026, 8, 2, 12, 0, 0),
    )
    tracker.update(
        [
            rp_row("SERIAL-A", "/dev/a"),
            rp_row(None, "/dev/no-serial"),
            rp_row("SERIAL-B", "/dev/b"),
        ],
        datetime(2026, 8, 2, 12, 0, 3),
    )

    rendered = format_target_rows(tracker)

    assert "runtime" in rendered
    assert "SERIAL-A" in rendered
    assert "/dev/a" in rendered
    assert "connected before picker started" in rendered
    assert "SERIAL-B" in rendered
    assert "12:00:03" in rendered
    assert "missing hardware serial" in rendered
    assert rendered.count(">") == 1
    assert max(len(line) for line in rendered.splitlines()) <= 80


def test_formats_scanning_empty_and_inventory_error_states() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})

    assert "Scanning" in format_target_rows(tracker, scanning=True)

    tracker.update([], datetime(2026, 8, 2, 12, 0, 0))
    assert "No matching devices detected" in format_target_rows(tracker)
    assert "USB inventory failed" in format_target_rows(
        tracker,
        inventory_error="USB inventory failed",
    )
    assert "Retrying automatically" in format_target_rows(
        tracker,
        inventory_error="USB inventory failed",
    )


def test_inventory_error_blocks_confirmation_of_stale_selection() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    tracker.update(
        [rp_row("SERIAL-A", "/dev/a")],
        datetime(2026, 8, 2, 12, 0, 0),
    )

    assert confirmation_serial(tracker, inventory_error=None) == "SERIAL-A"
    assert confirmation_serial(tracker, inventory_error="scan failed") is None


def test_windows_xterm_uses_vt100_output(monkeypatch) -> None:
    stderr = FakeTerminal(is_tty=True)
    expected_output = DummyOutput()
    vt100_calls = []

    monkeypatch.setattr(target_selector.sys, "platform", "win32")
    monkeypatch.setenv("TERM", "xterm-256color")
    monkeypatch.setattr(
        target_selector.Vt100_Output,
        "from_pty",
        lambda stream, term: vt100_calls.append((stream, term)) or expected_output,
    )
    monkeypatch.setattr(
        target_selector,
        "create_output",
        lambda **kwargs: (_ for _ in ()).throw(AssertionError(kwargs)),
    )

    assert target_selector.create_picker_output(stderr) is expected_output
    assert vt100_calls == [(stderr, "xterm-256color")]


def test_windows_native_terminal_uses_default_output(monkeypatch) -> None:
    stderr = FakeTerminal(is_tty=True)
    expected_output = DummyOutput()
    default_calls = []

    monkeypatch.setattr(target_selector.sys, "platform", "win32")
    monkeypatch.delenv("TERM", raising=False)
    monkeypatch.setattr(
        target_selector,
        "create_output",
        lambda **kwargs: default_calls.append(kwargs) or expected_output,
    )

    assert target_selector.create_picker_output(stderr) is expected_output
    assert default_calls == [{"stdout": stderr}]


class FakeTerminal(StringIO):
    def __init__(self, *, is_tty: bool) -> None:
        super().__init__()
        self._is_tty = is_tty

    def isatty(self) -> bool:
        return self._is_tty


def test_cli_rejects_non_tty_without_starting_picker() -> None:
    picker_calls = []
    stdout = StringIO()
    stderr = StringIO()

    result = main(
        ["--board", RP_BOARD, "--mode", "runtime"],
        stdin=FakeTerminal(is_tty=False),
        stdout=stdout,
        stderr=stderr,
        picker=lambda **kwargs: picker_calls.append(kwargs) or "SERIAL-A",
    )

    assert result == 2
    assert stdout.getvalue() == ""
    assert "SERIAL=<hardware serial>" in stderr.getvalue()
    assert picker_calls == []


def test_cli_prints_only_selected_serial_to_stdout() -> None:
    stdout = StringIO()
    stderr = FakeTerminal(is_tty=True)

    result = main(
        ["--board", RP_BOARD, "--mode", "runtime", "--mode", "bootloader"],
        stdin=FakeTerminal(is_tty=True),
        stdout=stdout,
        stderr=stderr,
        picker=lambda **kwargs: "SERIAL-A",
    )

    assert result == 0
    assert stdout.getvalue() == "SERIAL-A\n"
    assert stderr.getvalue() == ""


def test_cli_cancellation_prints_no_serial() -> None:
    stdout = StringIO()

    result = main(
        ["--board", RP_BOARD, "--mode", "runtime"],
        stdin=FakeTerminal(is_tty=True),
        stdout=stdout,
        stderr=FakeTerminal(is_tty=True),
        picker=lambda **kwargs: None,
    )

    assert result == 130
    assert stdout.getvalue() == ""


def test_picker_refreshes_until_a_new_device_can_be_confirmed() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    snapshots = iter([[], [rp_row("SERIAL-A", "/dev/a")]])
    times = iter(
        [
            datetime(2026, 8, 2, 12, 0, 0),
            datetime(2026, 8, 2, 12, 0, 1),
        ]
    )
    latest_time = datetime(2026, 8, 2, 12, 0, 1)

    async def inventory() -> list[tuple[str, tuple[int, int], str, str | None, str | None]]:
        return next(snapshots, [rp_row("SERIAL-A", "/dev/a")])

    async def exercise_picker() -> str | None:
        with create_pipe_input() as prompt_input:
            async def confirm_when_ready() -> None:
                while tracker.selected() is None:
                    await asyncio.sleep(0.001)
                prompt_input.send_text("\r")

            confirm_task = asyncio.create_task(confirm_when_ready())
            result = await run_picker_async(
                tracker=tracker,
                inventory=inventory,
                clock=lambda: next(times, latest_time),
                stdin=FakeTerminal(is_tty=True),
                stderr=FakeTerminal(is_tty=True),
                refresh_interval=0.001,
                prompt_input=prompt_input,
                prompt_output=DummyOutput(),
            )
            await confirm_task
            return result

    assert asyncio.run(exercise_picker()) == "SERIAL-A"
    assert tracker.selected().connected_at == datetime(2026, 8, 2, 12, 0, 1)


def test_picker_does_not_confirm_hidden_selection_after_scan_error() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    scan_count = 0
    error_observed = asyncio.Event()

    async def inventory() -> list[tuple[str, tuple[int, int], str, str | None, str | None]]:
        nonlocal scan_count
        scan_count += 1
        if scan_count == 1:
            return [rp_row("SERIAL-A", "/dev/a")]
        error_observed.set()
        raise RuntimeError("scan failed")

    async def exercise_picker() -> str | None:
        with create_pipe_input() as prompt_input:
            async def attempt_stale_confirmation_then_cancel() -> None:
                await error_observed.wait()
                await asyncio.sleep(0.01)
                prompt_input.send_text("\r")
                await asyncio.sleep(0.01)
                prompt_input.send_text("q")

            interaction = asyncio.create_task(attempt_stale_confirmation_then_cancel())
            result = await run_picker_async(
                tracker=tracker,
                inventory=inventory,
                clock=lambda: datetime(2026, 8, 2, 12, 0, scan_count),
                stdin=FakeTerminal(is_tty=True),
                stderr=FakeTerminal(is_tty=True),
                refresh_interval=0.001,
                prompt_input=prompt_input,
                prompt_output=DummyOutput(),
            )
            await interaction
            return result

    assert asyncio.run(exercise_picker()) is None


def test_picker_cancellation_cancels_an_in_progress_scan() -> None:
    tracker = TargetTracker(RP_BOARD, {"runtime"})
    scan_started = asyncio.Event()
    scan_cancelled = asyncio.Event()

    async def inventory() -> list[tuple[str, tuple[int, int], str, str | None, str | None]]:
        scan_started.set()
        try:
            await asyncio.sleep(60)
        finally:
            scan_cancelled.set()
        return []

    async def exercise_picker() -> str | None:
        with create_pipe_input() as prompt_input:
            async def cancel_when_scanning() -> None:
                await scan_started.wait()
                prompt_input.send_text("q")

            interaction = asyncio.create_task(cancel_when_scanning())
            result = await asyncio.wait_for(
                run_picker_async(
                    tracker=tracker,
                    inventory=inventory,
                    clock=lambda: datetime(2026, 8, 2, 12, 0, 0),
                    stdin=FakeTerminal(is_tty=True),
                    stderr=FakeTerminal(is_tty=True),
                    prompt_input=prompt_input,
                    prompt_output=DummyOutput(),
                ),
                timeout=0.2,
            )
            await interaction
            return result

    assert asyncio.run(exercise_picker()) is None
    assert scan_cancelled.is_set()
