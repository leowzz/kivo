#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
import sys
from collections.abc import Mapping, Sequence
from pathlib import Path

TAG_VERSION_RE = re.compile(r"^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
TRACKED_VERSION_FILES = (
    ".env.example",
    "package.json",
    "package-lock.json",
    "pyproject.toml",
    "uv.lock",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
    "src-tauri/tauri.conf.json",
)


class VersionError(ValueError):
    pass


def validate_tag_version(value: str) -> str:
    if TAG_VERSION_RE.fullmatch(value) is None:
        raise VersionError(f"expected vX.Y.Z, got {value!r}")
    return value


def numeric_version(tag: str) -> str:
    return validate_tag_version(tag)[1:]


def bump_patch(tag: str) -> str:
    match = TAG_VERSION_RE.fullmatch(validate_tag_version(tag))
    assert match is not None
    major, minor, patch = (int(part) for part in match.groups())
    return f"v{major}.{minor}.{patch + 1}"


def read_env_version(path: Path) -> str:
    try:
        lines = path.read_text().splitlines()
    except FileNotFoundError as error:
        raise VersionError(
            f"missing {path}; run: cp .env.example .env "
            "(PowerShell: Copy-Item .env.example .env)"
        ) from error
    if len(lines) != 1 or not lines[0].startswith("version="):
        raise VersionError(f"{path} must contain exactly one version=vX.Y.Z line")
    return validate_tag_version(lines[0].removeprefix("version="))


def resolve_firmware_build_id(root: Path, environ: Mapping[str, str]) -> str:
    explicit = environ.get("KIVO_FIRMWARE_BUILD_ID")
    if explicit is not None:
        if not explicit or re.fullmatch(r"\S+", explicit) is None:
            raise VersionError(
                "KIVO_FIRMWARE_BUILD_ID must be one non-whitespace token"
            )
        return explicit
    return read_env_version(root / ".env")


def _section_bounds(lines: list[str], header: str, path: Path) -> tuple[int, int]:
    starts = [index for index, line in enumerate(lines) if line.rstrip() == header]
    if len(starts) != 1:
        raise VersionError(f"{path}: expected exactly one {header} section")
    start = starts[0] + 1
    end = next(
        (index for index in range(start, len(lines)) if lines[index].startswith("[")),
        len(lines),
    )
    return start, end


def _package_bounds(
    lines: list[str], path: Path, *, require_virtual_source: bool
) -> tuple[int, int]:
    starts = [
        index for index, line in enumerate(lines) if line.rstrip() == "[[package]]"
    ]
    candidates: list[tuple[int, int]] = []
    for position, block_start in enumerate(starts):
        block_end = starts[position + 1] if position + 1 < len(starts) else len(lines)
        block = "".join(lines[block_start:block_end])
        if re.search(r'^name = "kivo"$', block, re.MULTILINE) is None:
            continue
        if require_virtual_source and 'source = { virtual = "." }' not in block:
            continue
        candidates.append((block_start + 1, block_end))
    if len(candidates) != 1:
        raise VersionError(f"{path}: expected exactly one local kivo package")
    return candidates[0]


def _version_line(lines: list[str], start: int, end: int, path: Path) -> int:
    matches = [
        index
        for index in range(start, end)
        if re.fullmatch(r'version = "[^"]+"\n?', lines[index])
    ]
    if len(matches) != 1:
        raise VersionError(f"{path}: expected exactly one version field")
    return matches[0]


def _read_json(path: Path) -> object:
    try:
        return json.loads(path.read_text())
    except (FileNotFoundError, json.JSONDecodeError) as error:
        raise VersionError(f"{path}: invalid or missing JSON") from error


def _json_content(document: object) -> str:
    return json.dumps(document, indent=2) + "\n"


def _read_section_version(path: Path, header: str) -> str:
    lines = path.read_text().splitlines(keepends=True)
    start, end = _section_bounds(lines, header, path)
    line = _version_line(lines, start, end, path)
    return lines[line].split('"')[1]


def _updated_section_content(path: Path, header: str, version: str) -> str:
    lines = path.read_text().splitlines(keepends=True)
    start, end = _section_bounds(lines, header, path)
    line = _version_line(lines, start, end, path)
    lines[line] = f'version = "{version}"\n'
    return "".join(lines)


def _read_package_version(path: Path, *, require_virtual_source: bool) -> str:
    lines = path.read_text().splitlines(keepends=True)
    start, end = _package_bounds(
        lines, path, require_virtual_source=require_virtual_source
    )
    line = _version_line(lines, start, end, path)
    return lines[line].split('"')[1]


def _updated_package_content(
    path: Path, version: str, *, require_virtual_source: bool
) -> str:
    lines = path.read_text().splitlines(keepends=True)
    start, end = _package_bounds(
        lines, path, require_virtual_source=require_virtual_source
    )
    line = _version_line(lines, start, end, path)
    lines[line] = f'version = "{version}"\n'
    return "".join(lines)


def _read_tauri_version(path: Path) -> str:
    document = _read_json(path)
    if not isinstance(document, dict) or not isinstance(document.get("version"), str):
        raise VersionError(f"{path}: root version must be a string")
    return document["version"]


def _updated_tauri_content(path: Path, version: str) -> str:
    _read_tauri_version(path)
    lines = path.read_text().splitlines(keepends=True)
    matches = [
        index
        for index, line in enumerate(lines)
        if re.fullmatch(r'(\s*"version"\s*:\s*")[^"]+("\s*,?\s*)', line)
    ]
    if len(matches) != 1:
        raise VersionError(f"{path}: expected exactly one root version field")
    index = matches[0]
    lines[index] = re.sub(
        r'^(\s*"version"\s*:\s*")[^"]+("\s*,?\s*)$',
        rf'\g<1>{version}\g<2>',
        lines[index],
    )
    return "".join(lines)


def _read_versions(root: Path) -> dict[str, str]:
    package_json = _read_json(root / "package.json")
    package_lock = _read_json(root / "package-lock.json")
    if not isinstance(package_json, dict) or not isinstance(
        package_json.get("version"), str
    ):
        raise VersionError("package.json: root version must be a string")
    if not isinstance(package_lock, dict) or not isinstance(
        package_lock.get("version"), str
    ):
        raise VersionError("package-lock.json: root version must be a string")
    packages = package_lock.get("packages")
    if not isinstance(packages, dict) or not isinstance(packages.get(""), dict):
        raise VersionError("package-lock.json: packages root entry is required")
    lock_root = packages[""]
    if not isinstance(lock_root.get("version"), str):
        raise VersionError("package-lock.json: packages root version must be a string")
    return {
        ".env.example": read_env_version(root / ".env.example")[1:],
        "package.json": package_json["version"],
        "package-lock.json": package_lock["version"],
        "package-lock.json:packages[\"\"]": lock_root["version"],
        "pyproject.toml": _read_section_version(root / "pyproject.toml", "[project]"),
        "uv.lock": _read_package_version(root / "uv.lock", require_virtual_source=True),
        "src-tauri/Cargo.toml": _read_section_version(
            root / "src-tauri" / "Cargo.toml", "[package]"
        ),
        "src-tauri/Cargo.lock": _read_package_version(
            root / "src-tauri" / "Cargo.lock", require_virtual_source=False
        ),
        "src-tauri/tauri.conf.json": _read_tauri_version(
            root / "src-tauri" / "tauri.conf.json"
        ),
    }


def set_repo_version(root: Path, tag: str) -> None:
    tag = validate_tag_version(tag)
    version = numeric_version(tag)
    package_json_path = root / "package.json"
    package_json = _read_json(package_json_path)
    if not isinstance(package_json, dict):
        raise VersionError("package.json: root must be an object")
    package_json["version"] = version

    package_lock_path = root / "package-lock.json"
    package_lock = _read_json(package_lock_path)
    if not isinstance(package_lock, dict) or not isinstance(
        package_lock.get("packages"), dict
    ) or not isinstance(package_lock["packages"].get(""), dict):
        raise VersionError("package-lock.json: packages root entry is required")
    package_lock["version"] = version
    package_lock["packages"][""]["version"] = version
    contents = {
        root / ".env": f"version={tag}\n",
        root / ".env.example": f"version={tag}\n",
        package_json_path: _json_content(package_json),
        package_lock_path: _json_content(package_lock),
        root / "pyproject.toml": _updated_section_content(
            root / "pyproject.toml", "[project]", version
        ),
        root / "uv.lock": _updated_package_content(
            root / "uv.lock", version, require_virtual_source=True
        ),
        root / "src-tauri" / "Cargo.toml": _updated_section_content(
            root / "src-tauri" / "Cargo.toml", "[package]", version
        ),
        root / "src-tauri" / "Cargo.lock": _updated_package_content(
            root / "src-tauri" / "Cargo.lock", version, require_virtual_source=False
        ),
        root / "src-tauri" / "tauri.conf.json": _updated_tauri_content(
            root / "src-tauri" / "tauri.conf.json", version
        ),
    }
    for path, content in contents.items():
        path.write_text(content)


def check_repo_version(root: Path, expected_tag: str | None = None) -> str:
    env_tag = read_env_version(root / ".env")
    expected_tag = (
        validate_tag_version(expected_tag) if expected_tag is not None else env_tag
    )
    expected_version = numeric_version(expected_tag)
    values = _read_versions(root)
    inconsistent = [
        path for path, value in values.items() if value != expected_version
    ]
    if env_tag != expected_tag:
        inconsistent.insert(0, ".env")
    if inconsistent:
        raise VersionError(
            f"repository version {expected_tag} mismatch: {', '.join(inconsistent)}"
        )
    return expected_tag


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    commands = parser.add_subparsers(dest="command", required=True)
    get = commands.add_parser("get")
    get.add_argument("--numeric", action="store_true")
    get.add_argument("--bump-patch", action="store_true")
    set_command = commands.add_parser("set")
    set_command.add_argument("tag")
    check = commands.add_parser("check")
    check.add_argument("tag", nargs="?")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        if args.command == "get":
            tag = read_env_version(args.root / ".env")
            if args.bump_patch:
                tag = bump_patch(tag)
            print(numeric_version(tag) if args.numeric else tag)
        elif args.command == "set":
            set_repo_version(args.root, args.tag)
        elif args.command == "check":
            check_repo_version(args.root, args.tag)
    except VersionError as error:
        print(error, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
