# Repository-Versioned Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `make release` synchronize every Kivo-owned version, commit the synchronized files, and create the annotated tag only after the commit succeeds.

**Architecture:** A dependency-free `scripts/repo_version.py` module owns version parsing, mutation, and consistency checks. `scripts/release.sh` is a narrow Git transaction orchestrator around that module; Make/PlatformIO consume `.env` for local firmware IDs, while CI and README establish `.env` on fresh checkouts.

**Tech Stack:** Bash, Python 3.13 standard library, GNU/BSD Make, PlatformIO/SCons, pytest, Git, GitHub Actions YAML, Markdown.

## Global Constraints

- `.env` and `.env.example` use exact tag form `version=vX.Y.Z`; package metadata uses `X.Y.Z`.
- `.env` stays ignored and is the local release input; `.env.example` is the tracked bootstrap template.
- Local firmware build IDs equal the complete `.env` value and never add `+dev`.
- Explicit non-whitespace `KIVO_FIRMWARE_BUILD_ID` and `BUILD_ID` overrides remain supported.
- A staged, unstaged, or non-ignored untracked path makes `make release` fail before mutation.
- Release ordering is validate, update, check, explicit stage, optional release commit, annotated tag.
- The release command never pushes and never creates a tag after a failed update or commit.
- Only Kivo-owned version fields change; dependency packages that also use `0.1.0` remain unchanged.
- Do not execute `make release` in the live checkout during implementation verification; release tests use temporary Git repositories.
- Every shell command run by the agent is prefixed with `rtk`.

## File Map

- Create `scripts/repo_version.py`: shared version parser, updater, checker, firmware fallback, and CLI.
- Create `test/version_fixtures.py`: minimal repository fixture shared by version and release tests.
- Create `test/test_repo_version.py`: unit and CLI coverage for repository version synchronization.
- Create `test/test_release_transaction.py`: real temporary-Git coverage for commit/tag ordering and rejection paths.
- Create `test/test_platformio_build_id.py`: PlatformIO adapter and `.env` fallback coverage.
- Modify `scripts/release.sh`: clean-tree release transaction.
- Modify `Makefile`: `.env`-backed `BUILD_ID`, validation prerequisite, and new test files in the complete gate.
- Modify `scripts/platformio_build_id.py`: explicit environment override followed by `.env` fallback.
- Modify `.github/workflows/release-windows.yml`: verify committed versions instead of rewriting Tauri configuration.
- Modify `.github/workflows/windows-ci.yml`: initialize `.env` from `.env.example` before native firmware tests.
- Modify `README.md`: document first-checkout `.env` setup and release behavior for Bash and PowerShell.
- Modify `test/test_release.sh`: static workflow, README, Makefile, and test-gate contract assertions.

---

### Task 1: Add The Repository Version Module

**Files:**
- Create: `scripts/repo_version.py`
- Create: `test/version_fixtures.py`
- Create: `test/test_repo_version.py`

**Interfaces:**
- Consumes: repository root `Path` and tag-form strings such as `v0.5.10`.
- Produces: `VersionError`, `TRACKED_VERSION_FILES`, `validate_tag_version()`, `read_env_version()`, `numeric_version()`, `bump_patch()`, `resolve_firmware_build_id()`, `set_repo_version()`, `check_repo_version()`, and CLI commands `get`, `set`, and `check`.

- [ ] **Step 1: Create the shared minimal repository fixture**

Create `test/version_fixtures.py` with `seed_version_repo(root: Path, tag: str = "v0.1.0") -> None`. It must create parent directories and write these exact logical fixtures:

```python
from __future__ import annotations

import json
from pathlib import Path


def seed_version_repo(root: Path, tag: str = "v0.1.0") -> None:
    numeric = tag.removeprefix("v")
    (root / "src-tauri").mkdir(parents=True, exist_ok=True)
    (root / ".env").write_text(f"version={tag}\n")
    (root / ".env.example").write_text(f"version={tag}\n")
    (root / ".gitignore").write_text(".env\n")
    (root / "package.json").write_text(
        json.dumps({"name": "kivo", "private": True, "version": numeric}, indent=2)
        + "\n"
    )
    (root / "package-lock.json").write_text(
        json.dumps(
            {
                "name": "kivo",
                "version": numeric,
                "lockfileVersion": 3,
                "packages": {"": {"name": "kivo", "version": numeric}},
            },
            indent=2,
        )
        + "\n"
    )
    (root / "pyproject.toml").write_text(
        f'[project]\nname = "kivo"\nversion = "{numeric}"\n'
    )
    (root / "uv.lock").write_text(
        'version = 1\n\n[[package]]\nname = "dependency"\nversion = "0.1.0"\n'
        f'\n[[package]]\nname = "kivo"\nversion = "{numeric}"\n'
        'source = { virtual = "." }\n'
    )
    (root / "src-tauri" / "Cargo.toml").write_text(
        f'[package]\nname = "kivo"\nversion = "{numeric}"\nedition = "2024"\n'
    )
    (root / "src-tauri" / "Cargo.lock").write_text(
        'version = 4\n\n[[package]]\nname = "dependency"\nversion = "0.1.0"\n'
        f'\n[[package]]\nname = "kivo"\nversion = "{numeric}"\n'
    )
    (root / "src-tauri" / "tauri.conf.json").write_text(
        json.dumps(
            {"productName": "Kivo", "version": numeric, "bundle": {"targets": ["app"]}},
            indent=2,
        )
        + "\n"
    )
```

- [ ] **Step 2: Write failing parser and synchronization tests**

Create `test/test_repo_version.py` with focused tests that assert:

```python
from pathlib import Path

import pytest

from scripts.repo_version import (
    VersionError,
    bump_patch,
    check_repo_version,
    read_env_version,
    set_repo_version,
)
from version_fixtures import seed_version_repo


def test_set_repo_version_updates_kivo_fields_only(tmp_path: Path) -> None:
    seed_version_repo(tmp_path)

    set_repo_version(tmp_path, "v1.2.3")
    check_repo_version(tmp_path, "v1.2.3")

    assert (tmp_path / ".env").read_text() == "version=v1.2.3\n"
    assert (tmp_path / ".env.example").read_text() == "version=v1.2.3\n"
    assert 'name = "dependency"\nversion = "0.1.0"' in (
        tmp_path / "uv.lock"
    ).read_text()
    assert 'name = "dependency"\nversion = "0.1.0"' in (
        tmp_path / "src-tauri" / "Cargo.lock"
    ).read_text()


@pytest.mark.parametrize(
    "contents",
    ["", "version=0.1.0\n", "version=v01.2.3\n", "version=v1.2.3+dev\n",
     "version=v1.2.3\nversion=v1.2.4\n", "# comment\nversion=v1.2.3\n"],
)
def test_read_env_version_rejects_noncanonical_files(
    tmp_path: Path, contents: str
) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text(contents)
    with pytest.raises(VersionError):
        read_env_version(env_file)


def test_check_repo_version_names_inconsistent_file(tmp_path: Path) -> None:
    seed_version_repo(tmp_path)
    (tmp_path / "src-tauri" / "Cargo.toml").write_text(
        '[package]\nname = "kivo"\nversion = "9.9.9"\n'
    )
    with pytest.raises(VersionError, match="src-tauri/Cargo.toml"):
        check_repo_version(tmp_path, "v0.1.0")


def test_bump_patch_preserves_tag_form() -> None:
    assert bump_patch("v0.5.9") == "v0.5.10"
```

- [ ] **Step 3: Run the tests and verify the red state**

Run: `rtk pytest test/test_repo_version.py -q`

Expected: collection fails with `ModuleNotFoundError: No module named 'scripts.repo_version'`.

- [ ] **Step 4: Implement the dependency-free version module and CLI**

Create `scripts/repo_version.py` with these concrete rules:

```python
#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import sys
from typing import Mapping, Sequence

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
```

Implement block-aware parsing with these exact boundaries:

```python
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
```

Use `_section_bounds()` for `[project]` and `[package]`. Use
`_package_bounds(..., require_virtual_source=True)` for `uv.lock` and `False`
for `src-tauri/Cargo.lock`; the latter is still unique because the exact Kivo
name must occur once. `_version_line()` both reads and replaces only the field
inside the resolved block.

For JSON, parse each document before mutation. Set `package.json["version"]`,
`package-lock.json["version"]`, and
`package-lock.json["packages"][""]["version"]`, then serialize those two npm
files with two-space indentation and a final newline. For
`src-tauri/tauri.conf.json`, first verify parsed root `version` is a string,
then replace the single line matching
`^(\s*"version"\s*:\s*")[^"]+("\s*,?\s*)$`; this avoids reformatting compact
arrays elsewhere in the Tauri configuration.

`set_repo_version(root, tag)` updates the files in `TRACKED_VERSION_FILES` plus
`.env`. `check_repo_version(root, expected_tag=None)` reads `.env`, derives the
numeric form, collects every field above, and raises one `VersionError` listing
all paths whose values differ. It uses `expected_tag` when supplied and also
reports `.env` when the local input differs from that expected tag.

Implement the CLI exactly as:

```text
repo_version.py [--root PATH] get [--numeric] [--bump-patch]
repo_version.py [--root PATH] set vX.Y.Z
repo_version.py [--root PATH] check [vX.Y.Z]
```

`main()` prints `VersionError` messages to stderr and returns `2`; successful
`set` and `check` return `0` without extra output.

- [ ] **Step 5: Run focused tests and verify green**

Run: `rtk pytest test/test_repo_version.py -q`

Expected: all tests in `test/test_repo_version.py` pass.

- [ ] **Step 6: Verify formatting and commit Task 1**

Run:

```bash
rtk ruff check scripts/repo_version.py test/version_fixtures.py test/test_repo_version.py
rtk git diff --check
rtk git add scripts/repo_version.py test/version_fixtures.py test/test_repo_version.py
rtk git commit -m "feat: synchronize repository versions"
```

Expected: Ruff and whitespace checks pass; the commit contains only the three
Task 1 files.

---

### Task 2: Make Release A Commit-Then-Tag Transaction

**Files:**
- Create: `test/test_release_transaction.py`
- Modify: `scripts/release.sh`

**Interfaces:**
- Consumes: Task 1 CLI, clean Git checkout, `.env`, and optional `V=vX.Y.Z`.
- Produces: optional `chore: release vX.Y.Z` commit followed by annotated tag `vX.Y.Z`; no push.

- [ ] **Step 1: Write the real-Git release transaction tests**

Create `test/test_release_transaction.py`. Its fixture copies
`scripts/release.sh` and `scripts/repo_version.py` into a repository seeded by
`seed_version_repo()`, configures a local test identity, commits the initial
state, and invokes the copied script with `PYTHON=sys.executable`.

Include these tests with exact behavioral assertions:

```python
def test_patch_release_commits_versions_before_annotated_tag(release_repo):
    before = git(release_repo, "rev-parse", "HEAD").stdout.strip()
    result = run_release(release_repo)
    assert result.returncode == 0, result.stderr
    assert git(release_repo, "log", "-1", "--format=%s").stdout.strip() == (
        "chore: release v0.1.1"
    )
    head = git(release_repo, "rev-parse", "HEAD").stdout.strip()
    assert head != before
    assert git(release_repo, "rev-parse", "v0.1.1^{}").stdout.strip() == head
    assert git(release_repo, "cat-file", "-t", "v0.1.1").stdout.strip() == "tag"
    assert (release_repo / ".env").read_text() == "version=v0.1.1\n"


def test_explicit_release_uses_requested_version(release_repo):
    result = run_release(release_repo, version="v2.3.4")
    assert result.returncode == 0, result.stderr
    check_repo_version(release_repo, "v2.3.4")


@pytest.mark.parametrize("dirty_kind", ["staged", "unstaged", "untracked"])
def test_dirty_worktree_is_rejected_without_mutation(release_repo, dirty_kind):
    dirty_path = make_dirty(release_repo, dirty_kind)
    before_head = git(release_repo, "rev-parse", "HEAD").stdout.strip()
    before_env = (release_repo / ".env").read_text()
    result = run_release(release_repo)
    assert result.returncode != 0
    assert str(dirty_path.relative_to(release_repo)) in result.stderr
    assert git(release_repo, "rev-parse", "HEAD").stdout.strip() == before_head
    assert (release_repo / ".env").read_text() == before_env
    assert not git(release_repo, "tag", "--list", "v0.1.1").stdout.strip()


def test_existing_tag_is_rejected_before_mutation(release_repo):
    git(release_repo, "tag", "v0.1.1")
    before_env = (release_repo / ".env").read_text()
    result = run_release(release_repo)
    assert result.returncode != 0
    assert (release_repo / ".env").read_text() == before_env


def test_synchronized_explicit_release_tags_without_empty_commit(release_repo):
    before = git(release_repo, "rev-parse", "HEAD").stdout.strip()
    result = run_release(release_repo, version="v0.1.0")
    assert result.returncode == 0, result.stderr
    assert git(release_repo, "rev-parse", "HEAD").stdout.strip() == before
    assert git(release_repo, "rev-parse", "v0.1.0^{}").stdout.strip() == before
```

Also cover missing `.env`, invalid explicit `V`, and a failing commit hook; each
must assert that no target tag exists. Add a fake `git` executable at the front
of `PATH` that delegates every command except `git tag -a`, which exits `42`.
That tag-failure test must assert that the `chore: release v0.1.1` commit remains
at `HEAD`, the tag does not exist, and stderr reports the failed tag command.
The successful release test must additionally assert that
`git diff-tree --no-commit-id --name-only -r HEAD` equals the eight paths in
`TRACKED_VERSION_FILES` and does not contain `.env`.

- [ ] **Step 2: Run the transaction tests and verify the red state**

Run: `rtk pytest test/test_release_transaction.py -q`

Expected: tests fail because the current script updates only `.env`, creates no
release commit, and does not reject dirty paths.

- [ ] **Step 3: Replace the release script with the ordered transaction**

Implement `scripts/release.sh` around this exact control flow:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="${PYTHON:-python3}"
VERSION_TOOL="$ROOT/scripts/repo_version.py"
VERSION_FILES=(
  .env.example package.json package-lock.json pyproject.toml uv.lock
  src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
)
cd "$ROOT"

"$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" get >/dev/null

dirty="$(git status --porcelain --untracked-files=normal)"
if [[ -n "$dirty" ]]; then
  echo "release: worktree must be clean:" >&2
  printf '%s\n' "$dirty" >&2
  exit 1
fi

if [[ -n "${V:-}" ]]; then
  NEW_VERSION="${V//$'\r'/}"
else
  NEW_VERSION="$("$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" get --bump-patch)"
fi
if [[ ! "$NEW_VERSION" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "release: expected V=vX.Y.Z, got ${NEW_VERSION:-<empty>}" >&2
  exit 1
fi
if git show-ref --verify --quiet "refs/tags/$NEW_VERSION"; then
  echo "release: git tag already exists: $NEW_VERSION" >&2
  exit 1
fi

"$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" set "$NEW_VERSION"
"$PYTHON_BIN" "$VERSION_TOOL" --root "$ROOT" check "$NEW_VERSION"
git add -- "${VERSION_FILES[@]}"
if ! git diff --cached --quiet --; then
  git commit -m "chore: release $NEW_VERSION" -- "${VERSION_FILES[@]}"
fi
git tag -a "$NEW_VERSION" -m "release $NEW_VERSION"
echo "release: version=$NEW_VERSION committed and tagged"
```

Treat `PYTHON` as one executable path and quote it on every invocation; tests
set it to `sys.executable`. Keep tag creation as the last state-changing
command.

- [ ] **Step 4: Run the transaction tests and verify green**

Run: `rtk pytest test/test_release_transaction.py -q`

Expected: all temporary-repository release tests pass and the live checkout has
no new release tag.

- [ ] **Step 5: Inspect commit/tag ordering in one disposable fixture**

Run: `rtk pytest test/test_release_transaction.py::test_patch_release_commits_versions_before_annotated_tag -q -vv`

Expected: one passing test; its assertions prove the annotated tag resolves to
the release commit rather than the parent.

- [ ] **Step 6: Commit Task 2**

Run:

```bash
rtk git diff --check
rtk git add scripts/release.sh test/test_release_transaction.py
rtk git commit -m "feat: commit version updates before release tags"
```

Expected: the commit contains only the release orchestrator and its tests.

---

### Task 3: Resolve Firmware Versions From `.env`

**Files:**
- Create: `test/test_platformio_build_id.py`
- Modify: `scripts/platformio_build_id.py`
- Modify: `Makefile`
- Modify: `test/test_make_upload_selection.py`

**Interfaces:**
- Consumes: Task 1 `resolve_firmware_build_id()`, optional Make `BUILD_ID`, optional `KIVO_FIRMWARE_BUILD_ID`, and repository `.env`.
- Produces: exact firmware macro value such as `v0.5.10`, with explicit caller overrides preserved.

- [ ] **Step 1: Write failing PlatformIO resolution tests**

Create `test/test_platformio_build_id.py` with direct coverage of the shared
resolver and adapter:

```python
from pathlib import Path
import runpy

import pytest

from scripts.repo_version import VersionError, resolve_firmware_build_id


def test_firmware_build_id_defaults_to_env_without_dev_suffix(tmp_path: Path) -> None:
    (tmp_path / ".env").write_text("version=v1.2.3\n")
    assert resolve_firmware_build_id(tmp_path, {}) == "v1.2.3"


def test_explicit_firmware_build_id_takes_precedence(tmp_path: Path) -> None:
    (tmp_path / ".env").write_text("version=v1.2.3\n")
    assert resolve_firmware_build_id(
        tmp_path, {"KIVO_FIRMWARE_BUILD_ID": "acceptance-build"}
    ) == "acceptance-build"


def test_missing_env_has_onboarding_command(tmp_path: Path) -> None:
    with pytest.raises(VersionError, match="cp .env.example .env"):
        resolve_firmware_build_id(tmp_path, {})
```

Add an adapter test using `runpy.run_path()` with a fake SCons `env` object and
an `Import` function in `init_globals`; assert the appended define is exactly
the resolver result and no `+dev` appears.

- [ ] **Step 2: Extend Make tests for `.env` default and missing-file failure**

Change `run_make()` in `test/test_make_upload_selection.py` to accept
`build_id: str | None = "test-build"` and `env_file: Path | None = None`. Only
append `BUILD_ID=...` when non-null, and append `ENV_FILE=...` when supplied.
Make the fake `uv` log `KIVO_FIRMWARE_BUILD_ID` before its arguments.

Add:

```python
def test_make_uses_env_version_as_default_build_id(tmp_path: Path) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text("version=v1.2.3\n")
    result, invocations = run_make(
        tmp_path, "build-rp2040", build_id=None, env_file=env_file
    )
    assert result.returncode == 0, result.stderr
    assert any(line.startswith("v1.2.3|") for line in invocations)
    assert all("+dev" not in line for line in invocations)


def test_make_firmware_target_explains_missing_env(tmp_path: Path) -> None:
    result, invocations = run_make(
        tmp_path,
        "build-rp2040",
        build_id=None,
        env_file=tmp_path / "missing.env",
    )
    assert result.returncode != 0
    assert "cp .env.example .env" in result.stderr
    assert invocations == []
```

- [ ] **Step 3: Run focused tests and verify the red state**

Run:

```bash
rtk pytest test/test_platformio_build_id.py -q
rtk pytest test/test_make_upload_selection.py -q
```

Expected: the adapter still falls back to `0.1.0+dev`, and Make does not read
the supplied `.env` or reject a missing version.

- [ ] **Step 4: Implement Make and PlatformIO `.env` resolution**

At the top of `Makefile`, replace the hard-coded build ID with:

```make
ENV_FILE ?= .env
-include $(ENV_FILE)
BUILD_ID ?= $(version)
```

Add a phony `require-build-id` target that rejects an empty or whitespace build
ID and prints both `cp .env.example .env` and
`Copy-Item .env.example .env`. Make `build-esp32s3`, `build-rp2040`,
`upload-esp32s3`, and `upload-rp2040` depend on it. Preserve explicit
`BUILD_ID=test-build` behavior used by existing upload tests.

Replace `scripts/platformio_build_id.py` with:

```python
import os
from pathlib import Path
import sys

SCRIPT_DIR = Path(__file__).resolve().parent
ROOT = SCRIPT_DIR.parent
sys.path.insert(0, str(ROOT))

from scripts.repo_version import resolve_firmware_build_id  # noqa: E402

Import("env")  # type: ignore[name-defined]  # PlatformIO provides this symbol.

build_id = resolve_firmware_build_id(ROOT, os.environ)
env.Append(  # type: ignore[name-defined]
    CPPDEFINES=[("KIVO_FIRMWARE_BUILD_ID", env.StringifyMacro(build_id))]
)
```

- [ ] **Step 5: Run focused tests and native PlatformIO test**

Run:

```bash
rtk pytest test/test_platformio_build_id.py test/test_make_upload_selection.py -q
rtk proxy env KIVO_FIRMWARE_BUILD_ID=v0.5.9 uv run pio test -e native
```

Expected: all pytest cases pass; PlatformIO native tests pass with the explicit
tag and no `.env` dependency in that invocation.

- [ ] **Step 6: Commit Task 3**

Run:

```bash
rtk git diff --check
rtk git add Makefile scripts/platformio_build_id.py test/test_make_upload_selection.py test/test_platformio_build_id.py
rtk git commit -m "build: read firmware version from env file"
```

Expected: the commit contains only firmware version resolution and its tests.

---

### Task 4: Enforce The Version Contract In CI And Documentation

**Files:**
- Modify: `.github/workflows/release-windows.yml`
- Modify: `.github/workflows/windows-ci.yml`
- Modify: `README.md`
- Modify: `Makefile`
- Modify: `test/test_release.sh`

**Interfaces:**
- Consumes: Task 1 CLI and the committed `.env.example` version.
- Produces: fresh-checkout `.env` instructions, CI initialization, tag-to-manifest verification, and complete local test coverage.

- [ ] **Step 1: Add failing static contract assertions**

Extend `test/test_release.sh` with these assertions:

```bash
WINDOWS_WORKFLOW="$ROOT/.github/workflows/windows-ci.yml"

grep -Fq 'python scripts/repo_version.py check "${GITHUB_REF_NAME}"' \
  "$RELEASE_WORKFLOW"
! grep -Fq 'config.version = tag.slice(1)' "$RELEASE_WORKFLOW"
grep -Fq 'Copy-Item .env.example .env' "$WINDOWS_WORKFLOW"
grep -Fq 'cp .env.example .env' "$README"
grep -Fq 'Copy-Item .env.example .env' "$README"
grep -Fq 'version=vX.Y.Z' "$README"
grep -Fq 'test/test_repo_version.py' "$MAKEFILE"
grep -Fq 'test/test_release_transaction.py' "$MAKEFILE"
grep -Fq 'test/test_platformio_build_id.py' "$MAKEFILE"
```

Remove the obsolete bottom-of-file release fixture that expects the old script
to update only `.env` and tag without a commit; Task 2 pytest coverage replaces
it.

- [ ] **Step 2: Run the static release test and verify the red state**

Run: `rtk test bash test/test_release.sh`

Expected: failure at the first new workflow or README assertion.

- [ ] **Step 3: Update tagged release CI to verify instead of rewrite**

In `.github/workflows/release-windows.yml`, add Python 3.13 setup to the desktop
release job and replace `Set app version from tag` with:

```yaml
      - name: Verify repository version
        shell: bash
        run: python scripts/repo_version.py check "${GITHUB_REF_NAME}"
```

Do not change firmware's explicit `KIVO_FIRMWARE_BUILD_ID` assignment. Do not
write any manifest during CI.

- [ ] **Step 4: Initialize `.env` in Windows CI**

In `.github/workflows/windows-ci.yml`, insert this before `Test native firmware
core`:

```yaml
      - name: Initialize repository version
        run: Copy-Item .env.example .env
```

- [ ] **Step 5: Document first-checkout and release behavior**

In the Bash bootstrap in `README.md`, add `cp .env.example .env` immediately
after `cd kivo`. Add a paragraph stating that `.env` is intentionally ignored,
contains exactly `version=vX.Y.Z`, and supplies local firmware builds and
`make release`.

In the PowerShell bootstrap, add `Copy-Item .env.example .env` before dependency
installation. Add one concise release paragraph explaining that `make release`
bumps the patch by default, `make release V=vX.Y.Z` overrides it, a dirty tree
is rejected, tracked package versions are committed as
`chore: release vX.Y.Z`, and the annotated tag is created last.

- [ ] **Step 6: Add the new focused tests to `make test`**

Immediately after `bash test/test_release.sh`, add:

```make
	$(UV_CMD) run pytest test/test_repo_version.py test/test_release_transaction.py test/test_platformio_build_id.py
```

Keep all existing test commands and their order unchanged after this insertion.

- [ ] **Step 7: Run focused CI/documentation contract tests**

Run:

```bash
rtk test bash test/test_release.sh
rtk pytest test/test_repo_version.py test/test_release_transaction.py test/test_platformio_build_id.py -q
rtk git diff --check
```

Expected: every command exits zero.

- [ ] **Step 8: Commit Task 4**

Run:

```bash
rtk git add .github/workflows/release-windows.yml .github/workflows/windows-ci.yml README.md Makefile test/test_release.sh
rtk git commit -m "docs: establish repository version setup"
```

Expected: the commit contains only CI, README, complete-gate wiring, and static
contract changes.

---

### Task 5: Full Verification And Release-Safety Audit

**Files:**
- Verify only; modify a task-owned file only when a failing command exposes a defect.

**Interfaces:**
- Consumes: completed Tasks 1-4.
- Produces: fresh evidence that the complete repository gate passes and the live checkout has no accidental release tag or dirty files.

- [ ] **Step 1: Run every focused release/version test together**

Run:

```bash
rtk test bash test/test_release.sh
rtk pytest test/test_repo_version.py test/test_release_transaction.py test/test_platformio_build_id.py test/test_make_upload_selection.py -q
```

Expected: all shell and pytest release/version tests pass.

- [ ] **Step 2: Run the complete repository acceptance gate**

Run: `rtk direnv exec . make test`

Expected: release tests, Python tests, PlatformIO native tests, Cargo tests,
Clippy, frontend tests, and frontend production build all exit zero.

- [ ] **Step 3: Verify formatting, scope, and live Git safety**

Run:

```bash
rtk git diff --check
rtk git status --short --branch
rtk git tag --list v0.5.10
rtk git log --oneline -6
```

Expected: no whitespace errors; no uncommitted implementation files; no
`v0.5.10` tag was created in the live checkout; recent commits show the design
and four scoped implementation commits.

- [ ] **Step 4: Audit the implementation against the approved spec**

Read `docs/superpowers/specs/2026-08-10-repository-versioned-release-design.md`
and confirm each file in “Files Kept In Sync,” every failure behavior, README
onboarding, CI verification, and commit-before-tag ordering has a passing test.
Record any environment-only gap explicitly rather than treating a focused test
as a real GitHub release run.
