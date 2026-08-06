from types import SimpleNamespace

import pytest

from scripts.kill_helper import kill_posix_helper, kill_windows_helper


def test_windows_helper_kill_uses_noninteractive_powershell() -> None:
    calls: list[list[str]] = []

    def run(command: list[str], **_kwargs: object) -> object:
        calls.append(command)
        return SimpleNamespace(returncode=0, stderr="")

    kill_windows_helper(run=run)

    assert calls[0][:4] == [
        "powershell.exe",
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
    ]
    assert "Get-Process -Name kivo" in calls[0][-1]
    assert "Stop-Process -Id $item.Id" in calls[0][-1]
    assert calls[0][-1].endswith("exit 0")


def test_windows_helper_kill_surfaces_real_failures() -> None:
    runner = lambda *_args, **_kwargs: SimpleNamespace(
        returncode=1, stderr="Access is denied"
    )

    with pytest.raises(RuntimeError, match="Access is denied"):
        kill_windows_helper(run=runner)


def test_posix_helper_kill_terminates_each_exact_name_match() -> None:
    killed = []
    runner = lambda *_args, **_kwargs: SimpleNamespace(
        returncode=0, stdout="12\n34\n", stderr=""
    )

    kill_posix_helper(run=runner, kill=lambda pid, sig: killed.append((pid, sig)))

    assert [pid for pid, _sig in killed] == [12, 34]
