import json
from io import StringIO
from pathlib import Path

from scripts.select_product_firmware import (
    FirmwareTracker,
    ProductFirmware,
    format_firmware_rows,
    main,
    resolve_product_firmware,
    scan_product_firmwares,
)


def write_artifact(
    root: Path,
    *,
    product_version_id: str,
    build_id: str,
    board: str,
    filename: str,
) -> Path:
    directory = root / product_version_id / build_id
    directory.mkdir(parents=True)
    firmware = directory / filename
    firmware.write_bytes(b"firmware")
    (directory / "manifest.json").write_text(
        json.dumps(
            {
                "product_version_id": product_version_id,
                "board_profile_id": board,
                "build_id": build_id,
                "firmware_file": filename,
            }
        ),
        encoding="utf-8",
    )
    return firmware


def test_scan_uses_manifest_metadata_and_filters_board(tmp_path: Path) -> None:
    root = tmp_path / "products"
    first = write_artifact(
        root,
        product_version_id="product-b",
        build_id="v2",
        board="yd-rp2040",
        filename="firmware.uf2",
    )
    second = write_artifact(
        root,
        product_version_id="product-a",
        build_id="v1",
        board="yd-rp2040",
        filename="firmware.uf2",
    )
    write_artifact(
        root,
        product_version_id="esp-product",
        build_id="v1",
        board="yd-esp32-s3",
        filename="firmware.factory.bin",
    )

    artifacts = scan_product_firmwares(root, "yd-rp2040")

    assert [artifact.path for artifact in artifacts] == [second, first]
    assert artifacts[0].product_version_id == "product-a"
    assert artifacts[0].build_id == "v1"


def test_scan_falls_back_to_standard_product_path_without_manifest(tmp_path: Path) -> None:
    firmware = tmp_path / "products" / "product-a" / "dev" / "firmware.uf2"
    firmware.parent.mkdir(parents=True)
    firmware.write_bytes(b"firmware")

    artifacts = scan_product_firmwares(tmp_path / "products", "yd-rp2040")

    assert artifacts == [
        ProductFirmware(firmware, "yd-rp2040", "product-a", "dev")
    ]


def test_scan_rejects_manifest_metadata_that_does_not_match_directory(tmp_path: Path) -> None:
    firmware = write_artifact(
        tmp_path / "products",
        product_version_id="product-a",
        build_id="dev",
        board="yd-rp2040",
        filename="firmware.uf2",
    )
    manifest = firmware.parent / "manifest.json"
    data = json.loads(manifest.read_text(encoding="utf-8"))
    data["build_id"] = "other-build"
    manifest.write_text(json.dumps(data), encoding="utf-8")

    assert scan_product_firmwares(tmp_path / "products", "yd-rp2040") == []


def test_resolve_requires_a_scanned_artifact_under_root(tmp_path: Path) -> None:
    root = tmp_path / "products"
    firmware = write_artifact(
        root,
        product_version_id="product-a",
        build_id="dev",
        board="yd-rp2040",
        filename="firmware.uf2",
    )
    outside = tmp_path / "firmware.uf2"
    outside.write_bytes(b"firmware")

    assert resolve_product_firmware(root, "yd-rp2040", firmware) is not None
    try:
        resolve_product_firmware(root, "yd-rp2040", outside)
    except ValueError as error:
        assert "product artifact" in str(error)
    else:
        raise AssertionError("outside firmware path was accepted")


def test_explicit_firmware_does_not_require_a_terminal(tmp_path: Path) -> None:
    root = tmp_path / "products"
    firmware = write_artifact(
        root,
        product_version_id="product-a",
        build_id="dev",
        board="yd-rp2040",
        filename="firmware.uf2",
    )
    stdout = StringIO()
    stderr = StringIO()

    result = main(
        ["--root", str(root), "--board", "yd-rp2040", "--firmware", str(firmware)],
        stdin=StringIO(),
        stdout=stdout,
        stderr=stderr,
    )

    assert result == 0
    assert stdout.getvalue() == f"{firmware.resolve()}\n"
    assert stderr.getvalue() == ""


def test_interactive_selection_requires_a_terminal(tmp_path: Path) -> None:
    root = tmp_path / "products"
    write_artifact(
        root,
        product_version_id="product-a",
        build_id="dev",
        board="yd-rp2040",
        filename="firmware.uf2",
    )
    stderr = StringIO()

    result = main(
        ["--root", str(root), "--board", "yd-rp2040"],
        stdin=StringIO(),
        stdout=StringIO(),
        stderr=stderr,
    )

    assert result == 2
    assert "FIRMWARE=<path>" in stderr.getvalue()


def test_format_firmware_rows_includes_identity_and_path(tmp_path: Path) -> None:
    artifact = ProductFirmware(
        tmp_path / "firmware.uf2", "yd-rp2040", "product-a", "dev"
    )

    rendered = format_firmware_rows(FirmwareTracker([artifact]))

    assert "product-a" in rendered
    assert "build=dev" in rendered
    assert str(artifact.path) in rendered
