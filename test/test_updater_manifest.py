import base64
import importlib.util
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("updater_manifest", ROOT / "scripts/updater_manifest.py")
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)

SIGNATURE = base64.b64encode(b"untrusted comment: test signature\nRWQfixture\n").decode()


def package(directory, name):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / name).write_bytes(b"update package fixture")
    (directory / f"{name}.sig").write_text(SIGNATURE + "\n", encoding="utf-8")


def staged_packages(directory):
    for product in updater.PRODUCTS:
        for name in updater.package_names(product, "1.2.3").values():
            package(directory, name.replace(" ", "."))


def test_separate_feeds_use_signed_product_packages_and_universal_macos(tmp_path):
    staged_packages(tmp_path)
    feeds = updater.generate_manifests(tmp_path, "v1.2.3", "leowzz/kivo")
    assert set(feeds) == {"latest.json", "latest-studio.json"}
    for file, prefix in [("latest.json", "Kivo"), ("latest-studio.json", "Kivo.Product.Studio")]:
        feed = feeds[file]
        assert feed["version"] == "1.2.3"
        platforms = feed["platforms"]
        assert set(platforms) == {"windows-x86_64", "darwin-x86_64", "darwin-aarch64"}
        assert platforms["windows-x86_64"]["url"].endswith(f"/v1.2.3/{prefix}_1.2.3_x64-setup.exe")
        assert platforms["darwin-aarch64"] == platforms["darwin-x86_64"]
        assert platforms["darwin-aarch64"]["url"].endswith(f"/{prefix}.app.tar.gz")
        assert all(entry["signature"] == SIGNATURE for entry in platforms.values())


@pytest.mark.parametrize("platform", ["nsis", "macos"])
def test_staging_normalizes_github_names_and_preserves_signatures(tmp_path, platform):
    root = tmp_path / "bundle"
    names = updater.package_names("studio", "1.2.3")
    filename = names["windows" if platform == "nsis" else "macos"]
    package(root / platform, filename)
    destination = tmp_path / "assets"
    updater.stage_artifacts(root, destination, "v1.2.3", "studio")
    assert (destination / filename.replace(" ", ".")).is_file()
    assert updater.signature(destination / f'{filename.replace(" ", ".")}.sig') == SIGNATURE


def test_missing_platform_or_signature_prevents_manifest_publication(tmp_path):
    staged_packages(tmp_path)
    (tmp_path / "Kivo.Product.Studio.app.tar.gz").unlink()
    with pytest.raises(ValueError, match="Missing updater package"):
        updater.generate_manifests(tmp_path, "1.2.3", "leowzz/kivo")
    package(tmp_path, "Kivo.Product.Studio.app.tar.gz")
    (tmp_path / "Kivo.app.tar.gz.sig").write_text("", encoding="utf-8")
    with pytest.raises(ValueError, match="Empty update signature"):
        updater.generate_manifests(tmp_path, "1.2.3", "leowzz/kivo")


@pytest.mark.parametrize("version", ["1.2.3-beta", "01.2.3", "1.2", "../1.2.3"])
def test_invalid_release_version_is_rejected(tmp_path, version):
    with pytest.raises(ValueError, match="stable release version"):
        updater.generate_manifests(tmp_path, version, "leowzz/kivo")


@pytest.mark.parametrize("value", ["not base64!", "YWJj", ""])
def test_malformed_signature_is_rejected(tmp_path, value):
    sig = tmp_path / "fixture.sig"
    sig.write_text(value, encoding="utf-8")
    with pytest.raises(ValueError, match="signature"):
        updater.signature(sig)


def test_missing_signature_prevents_staging(tmp_path):
    root = tmp_path / "bundle"
    package(root / "nsis", "Kivo_1.2.3_x64-setup.exe")
    (root / "nsis/Kivo_1.2.3_x64-setup.exe.sig").unlink()
    with pytest.raises(FileNotFoundError):
        updater.stage_artifacts(root, tmp_path / "assets", "1.2.3", "app")
