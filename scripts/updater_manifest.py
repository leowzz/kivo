"""Stage signed Tauri updater packages and generate separate APP / Studio feeds."""

import argparse
import base64
import json
import re
import shutil
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

PRODUCTS = {"app": "Kivo", "studio": "Kivo Product Studio"}


def validate_version(version: str) -> str:
    version = version.removeprefix("v")
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version):
        raise ValueError("Expected a stable release version")
    return version


def package_names(product: str, version: str) -> dict[str, str]:
    name = PRODUCTS[product]
    return {
        "windows": f"{name}_{version}_x64-setup.exe",
        "macos": f"{name}.app.tar.gz",
    }


def signature(path: Path) -> str:
    value = path.read_text(encoding="utf-8").strip()
    if not value:
        raise ValueError(f"Empty update signature: {path.name}")
    try:
        decoded = base64.b64decode(value, validate=True).decode("utf-8")
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError(f"Invalid update signature: {path.name}") from error
    if not decoded.startswith("untrusted comment:"):
        raise ValueError(f"Invalid update signature: {path.name}")
    return value


def stage_artifacts(bundle_root: Path, destination: Path, version: str, product: str) -> None:
    version = validate_version(version)
    names = package_names(product, version)
    candidates = [bundle_root / "nsis" / names["windows"], bundle_root / "macos" / names["macos"]]
    found = [path for path in candidates if path.is_file()]
    if not found:
        raise ValueError(f"Missing updater package for {PRODUCTS[product]}")
    destination.mkdir(parents=True, exist_ok=True)
    for package in found:
        sig = package.with_name(package.name + ".sig")
        signature(sig)
        # GitHub normalizes spaces in uploaded asset names to dots.
        name = package.name.replace(" ", ".")
        shutil.copyfile(package, destination / name)
        shutil.copyfile(sig, destination / f"{name}.sig")


def generate_manifests(directory: Path, version: str, repository: str) -> dict[str, dict]:
    version = validate_version(version)
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repository):
        raise ValueError("Invalid GitHub repository")
    result = {}
    for product, name in PRODUCTS.items():
        packages = {}
        for platform, filename in package_names(product, version).items():
            filename = filename.replace(" ", ".")
            if not (directory / filename).is_file():
                raise ValueError(f"Missing updater package: {filename}")
            packages[platform] = {
                "url": f"https://github.com/{repository}/releases/download/v{version}/{quote(filename)}",
                "signature": signature(directory / f"{filename}.sig"),
            }
        result["latest.json" if product == "app" else "latest-studio.json"] = {
            "version": version,
            "notes": f"{name} v{version}",
            "pub_date": datetime.now(timezone.utc).isoformat(),
            "platforms": {
                "windows-x86_64": packages["windows"],
                "darwin-x86_64": packages["macos"],
                "darwin-aarch64": packages["macos"],
            },
        }
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    stage = commands.add_parser("stage")
    stage.add_argument("bundle_root", type=Path)
    stage.add_argument("destination", type=Path)
    stage.add_argument("--version", required=True)
    stage.add_argument("--product", choices=PRODUCTS, required=True)
    manifest = commands.add_parser("manifest")
    manifest.add_argument("directory", type=Path)
    manifest.add_argument("--version", required=True)
    manifest.add_argument("--repository", required=True)
    args = parser.parse_args()
    if args.command == "stage":
        stage_artifacts(args.bundle_root, args.destination, args.version, args.product)
    else:
        # Validate both complete feeds before writing either one.
        feeds = generate_manifests(args.directory, args.version, args.repository)
        for name, feed in feeds.items():
            (args.directory / name).write_text(json.dumps(feed, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
