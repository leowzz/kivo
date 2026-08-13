import os
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def run_make(
    tmp_path: Path,
    target: str,
    *,
    build_id: str | None = "test-build",
    env_file: Path | None = None,
    extra_environment: dict[str, str] | None = None,
    serial: str | None = None,
    selected_serial: str = "SELECTED-SERIAL",
    runtime_serial: str | None = None,
    selector_exit: int = 0,
) -> tuple[subprocess.CompletedProcess[str], list[str]]:
    log_path = tmp_path / "uv.log"
    fake_uv = tmp_path / "fake-uv"
    fake_uv.write_text(
        """#!/bin/sh
set -eu
printf '%s|%s\\n' "${KIVO_FIRMWARE_BUILD_ID-}" "$*" >> "$KIVO_TEST_LOG"
case " $* " in
  *" scripts/select_firmware_target.py "*)
    if [ "$KIVO_SELECTOR_EXIT" -ne 0 ]; then
      exit "$KIVO_SELECTOR_EXIT"
    fi
    printf '%s\\n' "$KIVO_SELECTOR_SERIAL"
    ;;
  *" scripts/enter_download_mode.py "*)
    printf '%s\\n' "/dev/fake-download"
    ;;
  *" scripts/upload_rp2040.py "*)
    printf '%s\\n' "$KIVO_RUNTIME_SERIAL"
    ;;
esac
"""
    )
    fake_uv.chmod(0o755)
    environment = os.environ | {
        "KIVO_TEST_LOG": str(log_path),
        "KIVO_SELECTOR_SERIAL": selected_serial,
        "KIVO_RUNTIME_SERIAL": runtime_serial or selected_serial,
        "KIVO_SELECTOR_EXIT": str(selector_exit),
    }
    environment.update(extra_environment or {})
    command = ["make", target, f"UV={fake_uv}"]
    if build_id is not None:
        command.append(f"BUILD_ID={build_id}")
    if env_file is not None:
        command.append(f"ENV_FILE={env_file}")
    if serial is not None:
        command.append(f"SERIAL={serial}")

    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )
    invocations = log_path.read_text().splitlines() if log_path.exists() else []
    return result, invocations


def test_make_uses_env_version_as_default_build_id(tmp_path: Path) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text("version=v1.2.3\n")
    result, invocations = run_make(
        tmp_path, "build-rp2040", build_id=None, env_file=env_file
    )
    assert result.returncode == 0, result.stderr
    assert any(line.startswith("v1.2.3|") for line in invocations)
    assert all("+dev" not in line for line in invocations)


@pytest.mark.parametrize(
    ("contents", "error"),
    [
        ("version=not-a-tag\n", "expected vX.Y.Z"),
        (
            "version=v1.2.3\nversion=v1.2.4\n",
            "must contain exactly one version=vX.Y.Z line",
        ),
        (
            "version=v1.2.3\nBUILD_ID=bypass\n",
            "must contain exactly one version=vX.Y.Z line",
        ),
        (
            "version=v1.2.3\nPYTHON=/usr/bin/true\n",
            "must contain exactly one version=vX.Y.Z line",
        ),
    ],
)
def test_make_rejects_invalid_env_before_firmware_tools(
    tmp_path: Path, contents: str, error: str
) -> None:
    env_file = tmp_path / ".env"
    env_file.write_text(contents)

    result, invocations = run_make(
        tmp_path, "build-rp2040", build_id=None, env_file=env_file
    )

    assert result.returncode != 0
    assert error in result.stderr
    assert invocations == []


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


def test_make_ignores_ambient_version_when_env_file_is_missing(
    tmp_path: Path,
) -> None:
    result, invocations = run_make(
        tmp_path,
        "build-rp2040",
        build_id=None,
        env_file=tmp_path / "missing.env",
        extra_environment={"version": "v9.9.9"},
    )
    assert result.returncode != 0
    assert "cp .env.example .env" in result.stderr
    assert invocations == []


def test_explicit_environment_build_id_does_not_require_env_file(
    tmp_path: Path,
) -> None:
    result, invocations = run_make(
        tmp_path,
        "build-rp2040",
        build_id=None,
        env_file=tmp_path / "missing.env",
        extra_environment={"BUILD_ID": "feature/custom+1"},
    )

    assert result.returncode == 0, result.stderr
    assert any(line.startswith("feature/custom+1|") for line in invocations)


def test_explicit_serial_bypasses_selector_and_reaches_rp2040_tools(
    tmp_path: Path,
) -> None:
    result, invocations = run_make(
        tmp_path,
        "upload-rp2040",
        serial="EXPLICIT-SERIAL",
        selected_serial="EXPLICIT-SERIAL",
    )

    assert result.returncode == 0, result.stderr
    assert not any("select_firmware_target.py" in line for line in invocations)
    assert any("pio run -e rp2040" in line for line in invocations)
    assert any(
        "scripts/upload_rp2040.py" in line
        and "--serial EXPLICIT-SERIAL" in line
        and "--firmware .pio/build/rp2040/firmware.uf2" in line
        for line in invocations
    )
    assert any(
        "verify_runtime_firmware.py" in line and "--serial EXPLICIT-SERIAL" in line
        for line in invocations
    )


def test_selector_failure_stops_before_build_or_upload(tmp_path: Path) -> None:
    result, invocations = run_make(
        tmp_path,
        "upload-rp2040",
        selector_exit=130,
    )

    assert result.returncode != 0
    assert len(invocations) == 2
    assert "scripts/kill_helper.py" in invocations[0]
    assert "select_firmware_target.py" in invocations[1]


def test_bootsel_selection_verifies_with_the_resolved_runtime_serial(
    tmp_path: Path,
) -> None:
    result, invocations = run_make(
        tmp_path,
        "upload-rp2040",
        selected_serial="BOOTSEL-SERIAL",
        runtime_serial="RUNTIME-SERIAL",
    )

    assert result.returncode == 0, result.stderr
    assert any(
        "scripts/upload_rp2040.py" in line and "--serial BOOTSEL-SERIAL" in line
        for line in invocations
    )
    assert any(
        "verify_runtime_firmware.py" in line and "--serial RUNTIME-SERIAL" in line
        for line in invocations
    )


@pytest.mark.parametrize(
    ("target", "selector_arguments", "build_invocation", "upload_invocation"),
    [
        (
            "upload-rp2040",
            "--board vccgnd-yd-rp2040 --mode runtime --mode bootloader",
            "pio run -e rp2040",
            "scripts/upload_rp2040.py",
        ),
        (
            "upload-esp32s3",
            "--board luatos-esp32s3-aio --mode runtime",
            "pio run -e esp32s3",
            "pio run -e esp32s3 -t upload",
        ),
    ],
)
def test_selected_serial_flows_through_build_upload_and_verification(
    tmp_path: Path,
    target: str,
    selector_arguments: str,
    build_invocation: str,
    upload_invocation: str,
) -> None:
    result, invocations = run_make(tmp_path, target)

    assert result.returncode == 0, result.stderr
    assert "scripts/kill_helper.py" in invocations[0]
    assert selector_arguments in invocations[1]
    build_index = next(
        index
        for index, line in enumerate(invocations)
        if build_invocation in line and "-t upload" not in line
    )
    upload_index = next(
        index for index, line in enumerate(invocations) if upload_invocation in line
    )
    verify_index = next(
        index
        for index, line in enumerate(invocations)
        if "verify_runtime_firmware.py" in line
        and "--serial SELECTED-SERIAL" in line
    )
    assert 1 < build_index < upload_index < verify_index
