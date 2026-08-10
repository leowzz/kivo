import os
import sys
from pathlib import Path

Import("env")  # type: ignore[name-defined]  # noqa: F821 - provided by PlatformIO.

ROOT = Path(env.subst("$PROJECT_DIR")).resolve()  # type: ignore[name-defined]  # noqa: F821
sys.path.insert(0, str(ROOT))

from scripts.repo_version import resolve_firmware_build_id

build_id = resolve_firmware_build_id(ROOT, os.environ)
env.Append(  # type: ignore[name-defined]  # noqa: F821
    CPPDEFINES=[
        ("KIVO_FIRMWARE_BUILD_ID", env.StringifyMacro(build_id))  # noqa: F821
    ]
)
