from __future__ import annotations

import json
from pathlib import Path


def seed_version_repo(root: Path, tag: str = "v0.1.0") -> None:
    numeric = tag.removeprefix("v")
    (root / "src-tauri").mkdir(parents=True, exist_ok=True)
    (root / ".env").write_text(f"version={tag}\n")
    (root / ".env.example").write_text(f"version={tag}\n")
    (root / ".gitignore").write_text(".env\n")
    (root / "package.json").write_text(
        json.dumps({"name": "kivo", "private": True, "version": numeric}, indent=2)
        + "\n"
    )
    (root / "package-lock.json").write_text(
        json.dumps(
            {
                "name": "kivo",
                "version": numeric,
                "lockfileVersion": 3,
                "packages": {"": {"name": "kivo", "version": numeric}},
            },
            indent=2,
        )
        + "\n"
    )
    (root / "pyproject.toml").write_text(
        f'[project]\nname = "kivo"\nversion = "{numeric}"\n'
    )
    (root / "uv.lock").write_text(
        'version = 1\n\n[[package]]\nname = "dependency"\nversion = "0.1.0"\n'
        f'\n[[package]]\nname = "kivo"\nversion = "{numeric}"\n'
        'source = { virtual = "." }\n'
    )
    (root / "src-tauri" / "Cargo.toml").write_text(
        f'[package]\nname = "kivo"\nversion = "{numeric}"\nedition = "2024"\n'
    )
    (root / "src-tauri" / "Cargo.lock").write_text(
        'version = 4\n\n[[package]]\nname = "dependency"\nversion = "0.1.0"\n'
        f'\n[[package]]\nname = "kivo"\nversion = "{numeric}"\n'
    )
    (root / "src-tauri" / "tauri.conf.json").write_text(
        json.dumps(
            {"productName": "Kivo", "version": numeric, "bundle": {"targets": ["app"]}},
            indent=2,
        )
        + "\n"
    )
