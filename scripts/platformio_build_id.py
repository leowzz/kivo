import os
from pathlib import Path
import sys

Import("env")  # type: ignore[name-defined]  # PlatformIO provides this symbol.

ROOT = Path(env.subst("$PROJECT_DIR")).resolve()  # type: ignore[name-defined]
sys.path.insert(0, str(ROOT))

from scripts.repo_version import resolve_firmware_build_id  # noqa: E402

build_id = resolve_firmware_build_id(ROOT, os.environ)
env.Append(  # type: ignore[name-defined]
    CPPDEFINES=[("KIVO_FIRMWARE_BUILD_ID", env.StringifyMacro(build_id))]
)
