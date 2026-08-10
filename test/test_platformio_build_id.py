import os
from pathlib import Path
import runpy

import pytest

from scripts.repo_version import VersionError, resolve_firmware_build_id


ROOT = Path(__file__).resolve().parents[1]


class FakePlatformIOEnvironment:
    def __init__(self) -> None:
        self.cpp_defines: list[tuple[str, str]] = []

    def StringifyMacro(self, value: str) -> str:
        return f'"{value}"'

    def subst(self, variable: str) -> str:
        assert variable == "$PROJECT_DIR"
        return str(ROOT)

    def Append(self, *, CPPDEFINES: list[tuple[str, str]]) -> None:
        self.cpp_defines.extend(CPPDEFINES)


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


def test_platformio_adapter_appends_resolved_build_id(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("KIVO_FIRMWARE_BUILD_ID", "acceptance-build")
    expected = resolve_firmware_build_id(ROOT, os.environ)
    fake_env = FakePlatformIOEnvironment()

    runpy.run_path(
        str(ROOT / "scripts" / "platformio_build_id.py"),
        init_globals={"Import": lambda name: None, "env": fake_env},
    )

    assert fake_env.cpp_defines == [("KIVO_FIRMWARE_BUILD_ID", f'"{expected}"')]
    assert all("+dev" not in value for _, value in fake_env.cpp_defines)
