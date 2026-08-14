import os
from pathlib import Path

Import("env")  # type: ignore[name-defined]  # noqa: F821 - provided by PlatformIO.

generated = os.environ.get("KIVO_PRODUCT_GENERATED_DIR")
if generated:
    path = Path(generated).resolve()
    if not (path / "KivoProductGenerated.h").is_file():
        raise RuntimeError(f"missing generated Product Definition header: {path}")
    env.Append(CPPPATH=[str(path)])  # type: ignore[name-defined]  # noqa: F821
