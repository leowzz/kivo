import os
import re

Import("env")  # type: ignore[name-defined]  # PlatformIO provides this symbol.

build_id = os.environ.get("KIVO_FIRMWARE_BUILD_ID", "0.1.0+dev")
if not re.fullmatch(r"\S+", build_id):
    raise ValueError("KIVO_FIRMWARE_BUILD_ID must be one non-whitespace token")
env.Append(CPPDEFINES=[("KIVO_FIRMWARE_BUILD_ID", env.StringifyMacro(build_id))])
