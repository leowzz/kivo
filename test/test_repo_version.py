from pathlib import Path

import pytest
from version_fixtures import seed_version_repo

from scripts.repo_version import (
    TRACKED_VERSION_FILES,
    VersionError,
    bump_patch,
    check_repo_version,
    read_env_version,
    set_repo_version,
)

VERSION_FILES = (".env", *TRACKED_VERSION_FILES)


def snapshot_version_files(root: Path) -> dict[str, bytes | None]:
    return {
        relative_path: path.read_bytes() if path.exists() else None
        for relative_path in VERSION_FILES
        if (path := root / relative_path)
    }


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
    original_contents = snapshot_version_files(tmp_path)

    with pytest.raises(VersionError, match="root version must be a string"):
        set_repo_version(tmp_path, "v1.2.3")

    assert snapshot_version_files(tmp_path) == original_contents


@pytest.mark.parametrize(
    ("relative_path", "contents", "error"),
    [
        (
            "package.json",
            '{"name": "kivo"}\n',
            "package.json: root version must be a string",
        ),
        (
            "package.json",
            '{"name": "kivo", "version": 123}\n',
            "package.json: root version must be a string",
        ),
        (
            "package-lock.json",
            '{"packages": {"": {"version": "0.1.0"}}}\n',
            "package-lock.json: root version must be a string",
        ),
        (
            "package-lock.json",
            '{"version": "0.1.0", "packages": {"": {"version": 123}}}\n',
            "package-lock.json: packages root version must be a string",
        ),
    ],
)
def test_set_repo_version_rejects_invalid_npm_version_fields_without_writes(
    tmp_path: Path, relative_path: str, contents: str, error: str
) -> None:
    seed_version_repo(tmp_path)
    (tmp_path / relative_path).write_text(contents)
    original_contents = snapshot_version_files(tmp_path)

    with pytest.raises(VersionError, match=error):
        set_repo_version(tmp_path, "v1.2.3")

    assert snapshot_version_files(tmp_path) == original_contents


def test_set_repo_version_rejects_missing_env_example_without_writes(
    tmp_path: Path,
) -> None:
    seed_version_repo(tmp_path)
    (tmp_path / ".env.example").unlink()
    original_contents = snapshot_version_files(tmp_path)

    with pytest.raises(VersionError, match=r"missing .*\.env\.example"):
        set_repo_version(tmp_path, "v1.2.3")

    assert snapshot_version_files(tmp_path) == original_contents


@pytest.mark.parametrize(
    ("relative_path", "contents"),
    [
        (
            "package.json",
            '{"name": "kivo", "version": "0.1.0", "version": "9.9.9"}\n',
        ),
        (
            "package-lock.json",
            """{
  "version": "0.1.0",
  "packages": {
    "": {"version": "0.1.0", "version": "9.9.9"}
  }
}
""",
        ),
    ],
)
def test_set_repo_version_rejects_duplicate_json_version_keys_without_writes(
    tmp_path: Path, relative_path: str, contents: str
) -> None:
    seed_version_repo(tmp_path)
    (tmp_path / relative_path).write_text(contents)
    original_contents = snapshot_version_files(tmp_path)

    with pytest.raises(
        VersionError,
        match=rf"{relative_path.replace('.', r'\.')}: duplicate JSON key 'version'",
    ):
        set_repo_version(tmp_path, "v1.2.3")

    assert snapshot_version_files(tmp_path) == original_contents


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
