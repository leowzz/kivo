from pathlib import Path

import pytest

from scripts.repo_version import (
    TRACKED_VERSION_FILES,
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


def test_set_repo_version_does_not_partially_update_invalid_repository(
    tmp_path: Path,
) -> None:
    seed_version_repo(tmp_path)
    tauri_config = tmp_path / "src-tauri" / "tauri.conf.json"
    tauri_config.write_text('{"version": 123}\n')
    files = (".env", *TRACKED_VERSION_FILES)
    original_contents = {path: (tmp_path / path).read_text() for path in files}

    with pytest.raises(VersionError, match="root version must be a string"):
        set_repo_version(tmp_path, "v1.2.3")

    assert {
        path: (tmp_path / path).read_text() for path in files
    } == original_contents


@pytest.mark.parametrize(
    "contents",
    [
        "",
        "version=0.1.0\n",
        "version=v01.2.3\n",
        "version=v1.2.3+dev\n",
        "version=v1.2.3\nversion=v1.2.4\n",
        "# comment\nversion=v1.2.3\n",
    ],
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


def test_check_repo_version_rejects_empty_expected_tag(tmp_path: Path) -> None:
    seed_version_repo(tmp_path)

    with pytest.raises(VersionError, match="expected vX.Y.Z"):
        check_repo_version(tmp_path, "")


def test_bump_patch_preserves_tag_form() -> None:
    assert bump_patch("v0.5.9") == "v0.5.10"
