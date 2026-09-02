"""Select a product firmware artifact from output/products."""

import argparse
import asyncio
import json
import sys
from dataclasses import dataclass
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

try:
    from .select_firmware_target import create_picker_output
except ImportError:
    from select_firmware_target import create_picker_output


SUPPORTED_BOARDS = frozenset(("yd-rp2040", "yd-esp32-s3"))
BOARD_EXTENSIONS = {
    "yd-rp2040": frozenset((".uf2",)),
    "yd-esp32-s3": frozenset((".bin",)),
}


class FirmwareError(ValueError):
    """Raised when a product firmware path is not a valid artifact."""


@dataclass(frozen=True)
class ProductFirmware:
    path: Path
    board_profile_id: str
    product_version_id: str
    build_id: str


def _inside(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


def _artifact_matches_board(path: Path, board: str) -> bool:
    return path.suffix.lower() in BOARD_EXTENSIONS.get(board, frozenset())


def _manifest_firmware(manifest_path: Path, root: Path) -> ProductFirmware | None:
    try:
        directory_parts = manifest_path.parent.relative_to(root).parts
        if len(directory_parts) != 2:
            return None
    except ValueError:
        return None
    try:
        data = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return None
    if not isinstance(data, dict):
        return None
    fields = ("board_profile_id", "product_version_id", "build_id", "firmware_file")
    if not all(isinstance(data.get(field), str) and data[field] for field in fields):
        return None
    board = data["board_profile_id"]
    if (
        data["product_version_id"] != directory_parts[0]
        or data["build_id"] != directory_parts[1]
    ):
        return None
    firmware_file = data["firmware_file"]
    firmware_path = (manifest_path.parent / firmware_file).resolve()
    if (
        board not in SUPPORTED_BOARDS
        or Path(firmware_file).name != firmware_file
        or not _inside(firmware_path, root)
        or not firmware_path.is_file()
        or not _artifact_matches_board(firmware_path, board)
    ):
        return None
    return ProductFirmware(
        path=firmware_path,
        board_profile_id=board,
        product_version_id=data["product_version_id"],
        build_id=data["build_id"],
    )


def scan_product_firmwares(root: Path, board: str) -> list[ProductFirmware]:
    """Find product artifacts and associate them with their manifest metadata."""
    if board not in SUPPORTED_BOARDS:
        raise FirmwareError(f"unsupported product firmware board: {board}")
    root = root.expanduser().resolve()
    if not root.is_dir():
        return []

    found: dict[Path, ProductFirmware] = {}
    manifest_paths = list(root.rglob("manifest.json"))
    manifest_directories = {path.parent.resolve() for path in manifest_paths}
    for manifest_path in manifest_paths:
        artifact = _manifest_firmware(manifest_path, root)
        if artifact and artifact.board_profile_id == board:
            found[artifact.path] = artifact

    # Keep the command useful for artifacts copied without their manifest.
    for firmware_path in root.rglob("*"):
        if not firmware_path.is_file() or not _artifact_matches_board(firmware_path, board):
            continue
        firmware_path = firmware_path.resolve()
        if (
            firmware_path in found
            or firmware_path.parent in manifest_directories
            or not _inside(firmware_path, root)
        ):
            continue
        relative_parts = firmware_path.relative_to(root).parts
        if len(relative_parts) != 3:
            continue
        product_version_id, build_id = relative_parts[0:2]
        found[firmware_path] = ProductFirmware(
            path=firmware_path,
            board_profile_id=board,
            product_version_id=product_version_id,
            build_id=build_id,
        )
    return sorted(
        found.values(),
        key=lambda item: (item.product_version_id, item.build_id, str(item.path)),
    )


def resolve_product_firmware(
    root: Path, board: str, firmware: str | Path
) -> ProductFirmware:
    candidate = Path(firmware).expanduser().resolve()
    for artifact in scan_product_firmwares(root, board):
        if artifact.path == candidate:
            return artifact
    raise FirmwareError(
        f"firmware must be a {board} product artifact under {Path(root).expanduser().resolve()}"
    )


class FirmwareTracker:
    def __init__(self, artifacts: list[ProductFirmware]) -> None:
        self.artifacts = artifacts
        self.index = 0

    def move(self, delta: int) -> None:
        if self.artifacts:
            self.index = (self.index + delta) % len(self.artifacts)

    def selected(self) -> ProductFirmware | None:
        return self.artifacts[self.index] if self.artifacts else None


def format_firmware_rows(tracker: FirmwareTracker) -> str:
    lines = ["Kivo product firmware selector", ""]
    if not tracker.artifacts:
        lines.append("No product firmware artifacts found.")
    else:
        for index, artifact in enumerate(tracker.artifacts):
            marker = ">" if index == tracker.index else " "
            lines.append(
                f"{marker} {artifact.product_version_id}  build={artifact.build_id}"
            )
            lines.append(f"    board={artifact.board_profile_id}  file={artifact.path}")
            lines.append("")
    lines.extend(["", "Up/Down or j/k: select    Enter: confirm    q/Esc: cancel"])
    return "\n".join(lines)


async def run_picker_async(
    *,
    tracker: FirmwareTracker,
    stdin: TextIO,
    stderr: TextIO,
    prompt_input: Input | None = None,
    prompt_output: Output | None = None,
) -> ProductFirmware | None:
    bindings = KeyBindings()

    def render() -> str:
        return format_firmware_rows(tracker)

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
        selected = tracker.selected()
        if selected:
            event.app.exit(result=selected)

    @bindings.add("q")
    @bindings.add("escape")
    @bindings.add("c-c")
    def cancel(event: object) -> None:
        event.app.exit(result=None)

    owns_input = prompt_input is None
    application_input = create_input(stdin=stdin) if owns_input else prompt_input
    application_output = (
        create_picker_output(stderr) if prompt_output is None else prompt_output
    )
    application: Application[ProductFirmware | None] = Application(
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
    try:
        return await application.run_async()
    finally:
        if owns_input:
            application_input.close()


def run_picker(
    *, tracker: FirmwareTracker, stdin: TextIO, stderr: TextIO
) -> ProductFirmware | None:
    return asyncio.run(run_picker_async(tracker=tracker, stdin=stdin, stderr=stderr))


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Select a product firmware artifact.")
    parser.add_argument("--root", default="output/products", type=Path)
    parser.add_argument("--board", required=True, choices=sorted(SUPPORTED_BOARDS))
    parser.add_argument("--firmware", type=Path)
    return parser.parse_args(argv)


def main(
    argv: list[str] | None = None,
    *,
    stdin: TextIO | None = None,
    stdout: TextIO | None = None,
    stderr: TextIO | None = None,
) -> int:
    args = parse_args(argv)
    stdin = sys.stdin if stdin is None else stdin
    stdout = sys.stdout if stdout is None else stdout
    stderr = sys.stderr if stderr is None else stderr
    try:
        if args.firmware:
            selected = resolve_product_firmware(args.root, args.board, args.firmware)
        else:
            artifacts = scan_product_firmwares(args.root, args.board)
            if not artifacts:
                print(
                    f"No {args.board} product firmware found under {args.root}.",
                    file=stderr,
                )
                return 1
            if not stdin.isatty() or not stderr.isatty():
                print(
                    "Interactive product firmware selection requires a terminal; "
                    "pass FIRMWARE=<path>.",
                    file=stderr,
                )
                return 2
            selected = run_picker(
                tracker=FirmwareTracker(artifacts), stdin=stdin, stderr=stderr
            )
    except FirmwareError as error:
        print(error, file=stderr)
        return 1
    except KeyboardInterrupt:
        selected = None

    if selected is None:
        print("Product firmware selection cancelled.", file=stderr)
        return 130
    print(selected.path, file=stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
