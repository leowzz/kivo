import argparse
import sys
from collections.abc import Iterable, Sequence

try:
    from scripts.list_firmware_targets import cdc_rows, merge_rows
except ModuleNotFoundError:
    from list_firmware_targets import cdc_rows, merge_rows


InventoryRow = tuple[str, tuple[int, int], str, str | None, str | None]


class PortResolutionError(RuntimeError):
    pass


def resolve_runtime_port(
    rows: Iterable[InventoryRow],
    *,
    board: str,
    serial_number: str,
) -> str:
    matching_ports = list(
        dict.fromkeys(
            port
            for mode, _usb_id, observed_board, observed_serial, port in rows
            if mode == "runtime"
            and observed_board == board
            and observed_serial == serial_number
            and port
        )
    )
    if not matching_ports:
        raise PortResolutionError(
            f"runtime device {board} with serial {serial_number} was not found"
        )
    if len(matching_ports) > 1:
        raise PortResolutionError(
            f"runtime device {board} with serial {serial_number} has multiple ports: "
            + ", ".join(matching_ports)
        )
    return matching_ports[0]


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resolve a Kivo runtime device hardware serial to its CDC port."
    )
    parser.add_argument("--board", required=True)
    parser.add_argument("--serial", required=True)
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        port = resolve_runtime_port(
            merge_rows(cdc_rows()),
            board=args.board,
            serial_number=args.serial,
        )
    except PortResolutionError as error:
        print(f"resolve_firmware_port: {error}", file=sys.stderr)
        return 2
    print(port)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
