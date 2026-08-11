from __future__ import annotations

import os
import shlex
import shutil
import stat
import subprocess
import sys
from pathlib import Path

import pytest
from version_fixtures import seed_version_repo

from scripts.repo_version import TRACKED_VERSION_FILES, check_repo_version

PROJECT_ROOT = Path(__file__).resolve().parents[1]


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=repo,
        text=True,
        capture_output=True,
        check=check,
    )


@pytest.fixture
def release_repo(tmp_path: Path) -> Path:
    seed_version_repo(tmp_path)
    scripts = tmp_path / "scripts"
    scripts.mkdir()
    shutil.copy2(PROJECT_ROOT / "scripts" / "release.sh", scripts / "release.sh")
    shutil.copy2(
        PROJECT_ROOT / "scripts" / "repo_version.py", scripts / "repo_version.py"
    )
    git(tmp_path, "init", "-q")
    git(tmp_path, "config", "user.name", "test")
    git(tmp_path, "config", "user.email", "test@example.com")
    git(tmp_path, "add", ".")
    git(tmp_path, "commit", "-qm", "initial")
    return tmp_path


def run_release(
    repo: Path,
    *,
    version: str | None = None,
    path: str | None = None,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["PYTHON"] = sys.executable
    if version is not None:
        env["V"] = version
    if path is not None:
        env["PATH"] = path
    return subprocess.run(
        ["bash", "scripts/release.sh"],
        cwd=repo,
        text=True,
        capture_output=True,
        env=env,
        check=False,
    )


def make_dirty(repo: Path, dirty_kind: str) -> Path:
    if dirty_kind == "unstaged":
        path = repo / ".env.example"
        path.write_text("version=v9.9.9\n")
    else:
        path = repo / "dirty.txt"
        path.write_text("dirty\n")
        if dirty_kind == "staged":
            git(repo, "add", str(path))
    return path


def test_patch_release_commits_versions_before_annotated_tag(
    release_repo: Path,
) -> None:
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
    committed_paths = git(
        release_repo, "diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"
    ).stdout.splitlines()
    assert sorted(committed_paths) == sorted(TRACKED_VERSION_FILES)
    assert ".env" not in committed_paths


def test_explicit_release_uses_requested_version(release_repo: Path) -> None:
    result = run_release(release_repo, version="v2.3.4")
    assert result.returncode == 0, result.stderr
    check_repo_version(release_repo, "v2.3.4")


@pytest.mark.parametrize("dirty_kind", ["staged", "unstaged", "untracked"])
def test_dirty_worktree_is_rejected_without_mutation(
    release_repo: Path, dirty_kind: str
) -> None:
    dirty_path = make_dirty(release_repo, dirty_kind)
    before_head = git(release_repo, "rev-parse", "HEAD").stdout.strip()
    before_env = (release_repo / ".env").read_text()
    result = run_release(release_repo)
    assert result.returncode != 0
    assert str(dirty_path.relative_to(release_repo)) in result.stderr
    assert git(release_repo, "rev-parse", "HEAD").stdout.strip() == before_head
    assert (release_repo / ".env").read_text() == before_env
    assert not git(release_repo, "tag", "--list", "v0.1.1").stdout.strip()


def test_existing_tag_is_rejected_before_mutation(release_repo: Path) -> None:
    git(release_repo, "tag", "v0.1.1")
    before_env = (release_repo / ".env").read_text()
    result = run_release(release_repo)
    assert result.returncode != 0
    assert (release_repo / ".env").read_text() == before_env


def test_synchronized_explicit_release_tags_without_empty_commit(
    release_repo: Path,
) -> None:
    before = git(release_repo, "rev-parse", "HEAD").stdout.strip()
    result = run_release(release_repo, version="v0.1.0")
    assert result.returncode == 0, result.stderr
    assert git(release_repo, "rev-parse", "HEAD").stdout.strip() == before
    assert git(release_repo, "rev-parse", "v0.1.0^{}").stdout.strip() == before


def test_missing_env_is_rejected_without_tag(release_repo: Path) -> None:
    (release_repo / ".env").unlink()
    result = run_release(release_repo)
    assert result.returncode != 0
    assert not git(release_repo, "tag", "--list", "v0.1.1").stdout.strip()


def test_invalid_explicit_version_is_rejected_without_tag(
    release_repo: Path,
) -> None:
    before_env = (release_repo / ".env").read_text()
    result = run_release(release_repo, version="1.2.3")
    assert result.returncode != 0
    assert (release_repo / ".env").read_text() == before_env
    assert not git(release_repo, "tag", "--list", "1.2.3").stdout.strip()


def test_failing_commit_hook_does_not_create_tag(release_repo: Path) -> None:
    hook = release_repo / ".git" / "hooks" / "pre-commit"
    hook.write_text("#!/bin/sh\necho commit hook failed >&2\nexit 1\n")
    hook.chmod(hook.stat().st_mode | stat.S_IXUSR)

    result = run_release(release_repo)

    assert result.returncode != 0
    assert "commit hook failed" in result.stderr
    assert not git(release_repo, "tag", "--list", "v0.1.1").stdout.strip()


def test_failing_annotated_tag_leaves_release_commit_at_head(
    release_repo: Path, tmp_path_factory: pytest.TempPathFactory
) -> None:
    real_git = shutil.which("git")
    assert real_git is not None
    fake_bin = tmp_path_factory.mktemp("fake-git")
    fake_git = fake_bin / "git"
    fake_git.write_text(
        "#!/bin/sh\n"
        'if [ "$1" = tag ] && [ "$2" = -a ]; then\n'
        "  echo 'fake git: git tag -a failed' >&2\n"
        "  exit 42\n"
        "fi\n"
        f"exec {shlex.quote(real_git)} \"$@\"\n"
    )
    fake_git.chmod(fake_git.stat().st_mode | stat.S_IXUSR)

    result = run_release(
        release_repo, path=os.pathsep.join((str(fake_bin), os.environ["PATH"]))
    )

    assert result.returncode == 42
    assert git(release_repo, "log", "-1", "--format=%s").stdout.strip() == (
        "chore: release v0.1.1"
    )
    assert not git(release_repo, "tag", "--list", "v0.1.1").stdout.strip()
    assert "git tag -a failed" in result.stderr
