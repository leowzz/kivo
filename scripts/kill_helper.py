import os
import platform
import signal
import subprocess
from collections.abc import Callable


def kill_windows_helper(
    *, run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
) -> None:
    script = (
        "$ErrorActionPreference = 'Stop'; "
        "$items = @(Get-Process -Name kivo -ErrorAction SilentlyContinue); "
        "foreach ($item in $items) { "
        "try { Stop-Process -Id $item.Id -Force -ErrorAction Stop } "
        "catch { "
        "if (Get-Process -Id $item.Id -ErrorAction SilentlyContinue) { throw } "
        "} "
        "}; "
        "exit 0"
    )
    result = run(
        [
            "powershell.exe",
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        detail = result.stderr.strip() or f"exit {result.returncode}"
        raise RuntimeError(f"cannot stop Kivo: {detail}")


def kill_posix_helper(
    *,
    run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
    kill: Callable[[int, signal.Signals], None] = os.kill,
) -> None:
    result = run(
        ["pgrep", "-x", "kivo"],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode == 1:
        return
    if result.returncode:
        detail = result.stderr.strip() or f"exit {result.returncode}"
        raise RuntimeError(f"cannot find Kivo processes: {detail}")
    for value in result.stdout.splitlines():
        kill(int(value), signal.SIGTERM)


def main() -> None:
    try:
        if platform.system() == "Windows":
            kill_windows_helper()
        else:
            kill_posix_helper()
    except (OSError, RuntimeError, ValueError) as error:
        raise SystemExit(f"kill_helper: {error}") from error


if __name__ == "__main__":
    main()
