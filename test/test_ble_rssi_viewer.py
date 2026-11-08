import csv
from http.server import ThreadingHTTPServer
import io
import json
import threading
from types import SimpleNamespace
from urllib.error import HTTPError
from urllib.request import Request, urlopen

import pytest
import serial

from host.ble_rssi import viewer


PEER = "AA:BB:CC:DD:EE:FF"
SAMPLE_LINE = f"RSSI t_ms=123 peer={PEER} rssi_dbm=-62 source=connection"


@pytest.fixture
def history(tmp_path):
    history = viewer.History(tmp_path / "history.jsonl")
    yield history
    history.close()


def sample_records(history):
    return [r for r in history.snapshot(0)["records"] if r["type"] == "sample"]


def test_sample_uses_receive_time_and_normalizes_peer(history, monkeypatch):
    monkeypatch.setattr(viewer.time, "time", lambda: 1000.5)
    history.ingest(SAMPLE_LINE.lower().replace("rssi ", "RSSI ", 1))

    snapshot = history.snapshot(0)
    samples = sample_records(history)
    assert len(samples) == 1
    assert samples[0]["ts"] == 1000.5
    assert samples[0]["rssi"] == -62
    assert samples[0]["peer"] == PEER
    assert snapshot["state"]["last_sample_ts"] == 1000.5
    assert snapshot["state"]["last_rssi"] == -62
    assert snapshot["state"]["ble_state"] == "CONNECTED"


@pytest.mark.parametrize("line", [
    f"RSSI peer={PEER} rssi_dbm=127",
    f"RSSI peer={PEER} rssi_dbm=-128",
    f"RSSI peer={PEER} rssi_dbm=21",
    f"RSSI peer={PEER} rssi_dbm=bad",
    "RSSI rssi_dbm=-50",
    f"CANDIDATE peer={PEER} rssi_dbm=-50",
    "unrelated serial output",
])
def test_invalid_or_non_target_lines_do_not_create_samples(history, line):
    history.ingest(line)
    assert history.snapshot(0)["cursor"] == 0
    assert history.snapshot(0)["state"]["last_sample_ts"] is None


@pytest.mark.parametrize("rssi", [-127, 0, 20])
def test_valid_rssi_boundaries_are_not_treated_as_missing(history, rssi):
    history.ingest(f"RSSI mac={PEER} rssi_dbm={rssi}")
    assert history.snapshot(0)["state"]["last_rssi"] == rssi


def test_disconnect_preserves_old_sample_without_fabricating_a_new_reading(history):
    history.serial_state(True, "test-port")
    history.ingest(SAMPLE_LINE)
    before = history.snapshot(0)

    history.ingest("STATUS state=ADVERTISING")
    history.ingest("STATUS state=ADVERTISING")
    after_link = history.snapshot(before["cursor"])
    assert [r["type"] for r in after_link["records"]] == ["link"]
    assert after_link["state"]["ble_state"] == "ADVERTISING"

    history.serial_state(False, error="USB disconnected")
    after_usb = history.snapshot(after_link["cursor"])
    assert [r["type"] for r in after_usb["records"]] == ["serial"]
    assert after_usb["state"]["serial_connected"] is False
    assert after_usb["state"]["ble_state"] == "UNKNOWN"
    assert after_usb["state"]["last_sample_ts"] == before["state"]["last_sample_ts"]
    assert len(sample_records(history)) == 1


def test_unavailable_rssi_keeps_original_freshness_until_next_valid_reading(history):
    history.ingest(SAMPLE_LINE)
    before = history.snapshot(0)
    history.ingest("RSSI_UNAVAILABLE status=1")
    assert history.snapshot(0)["state"]["last_sample_ts"] == before["state"]["last_sample_ts"]
    assert history.snapshot(0)["state"]["error"]
    history.ingest(SAMPLE_LINE)
    assert history.snapshot(0)["state"]["error"] is None


def test_cursor_returns_only_new_records_and_markers_are_bounded(history):
    history.ingest(SAMPLE_LINE)
    cursor = history.snapshot(0)["cursor"]
    marker = history.marker("  门口  ")
    assert history.snapshot(cursor)["records"] == [marker]
    assert marker["label"] == "门口"
    assert history.marker(" ")["label"] == "位置标记"
    assert len(history.marker("字" * 100)["label"]) == 64


def test_history_limit_keeps_recent_records_with_a_monotonic_cursor(tmp_path):
    history = viewer.History(tmp_path / "history.jsonl", limit=2)
    try:
        history.marker("first")
        second = history.marker("second")
        third = history.marker("third")
        assert history.snapshot(0)["records"] == [second, third]
        assert history.snapshot(second["id"])["records"] == [third]
        assert history.cursor == third["id"]
    finally:
        history.close()


def test_history_recovers_partial_last_line_and_keeps_old_reading_stale(tmp_path):
    path = tmp_path / "history.jsonl"
    original = viewer.History(path)
    original.ingest(SAMPLE_LINE)
    saved = original.snapshot(0)
    original.close()
    with path.open("a") as stream:
        stream.write('{"id":')

    restored = viewer.History(path)
    assert restored.cursor == saved["cursor"]
    assert restored.state["last_sample_ts"] == saved["state"]["last_sample_ts"]
    assert restored.state["serial_connected"] is False
    assert restored.state["ble_state"] == "UNKNOWN"
    restored.marker("重启后")
    restored.close()

    reopened = viewer.History(path)
    assert reopened.cursor == saved["cursor"] + 1
    assert reopened.snapshot(0)["records"][-1]["label"] == "重启后"
    reopened.close()


def test_csv_keeps_unicode_and_quotes_formula_like_markers(history):
    history.ingest(SAMPLE_LINE)
    history.marker("  =2+2")
    history.marker("桌边，回来")
    rows = list(csv.DictReader(io.StringIO(history.csv_bytes().decode("utf-8-sig"))))
    assert rows[0]["type"] == "link"
    assert next(row for row in rows if row["type"] == "sample")["rssi_dbm"] == "-62"
    assert rows[-2]["label"] == "'=2+2"
    assert rows[-1]["label"] == "桌边，回来"
    assert rows[-1]["time"].endswith("+00:00")


@pytest.fixture
def http_server(history):
    server = ThreadingHTTPServer(("127.0.0.1", 0), viewer.make_handler(history))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{server.server_port}"
    server.shutdown()
    server.server_close()
    thread.join(timeout=2)


def request_json(url, payload=None):
    if payload is None:
        request = Request(url)
    else:
        request = Request(url, data=json.dumps(payload).encode(),
                          headers={"Content-Type": "application/json"})
    with urlopen(request, timeout=3) as response:
        return response.status, json.load(response)


@pytest.mark.parametrize("path,content_type", [
    ("/", "text/html"),
    ("/viewer.html", "text/html"),
    ("/assets/app.js", "text/javascript"),
    ("/assets/styles.css", "text/css"),
])
def test_static_assets_are_served_with_the_right_type(http_server, path, content_type):
    with urlopen(http_server + path, timeout=3) as response:
        assert response.status == 200
        assert response.headers["Content-Type"].startswith(content_type)
        assert response.read()


def test_http_history_marker_and_csv_contract(http_server, history):
    history.ingest(SAMPLE_LINE)
    _, initial = request_json(http_server + "/api/state")
    status, marker = request_json(http_server + "/api/marker", {"label": "门口"})
    assert status == 201
    _, delta = request_json(http_server + f"/api/state?after={initial['cursor']}")
    assert delta["records"] == [marker]
    assert delta["state"]["last_rssi"] == -62
    with urlopen(http_server + "/api/export.csv", timeout=3) as response:
        assert "attachment" in response.headers["Content-Disposition"]
        assert "门口" in response.read().decode("utf-8-sig")


@pytest.mark.parametrize("path", ["/api/state?after=-1", "/api/state?after=oops"])
def test_http_rejects_invalid_cursor(http_server, path):
    with pytest.raises(HTTPError) as error:
        urlopen(http_server + path, timeout=3)
    assert error.value.code == 400


@pytest.mark.parametrize("payload", [{"label": 123}, [], "marker"])
def test_http_rejects_invalid_marker_payload(http_server, history, payload):
    with pytest.raises(HTTPError) as error:
        request_json(http_server + "/api/marker", payload)
    assert error.value.code == 400
    assert history.cursor == 0


def test_static_routes_do_not_expose_other_files(http_server):
    with pytest.raises(HTTPError) as error:
        urlopen(http_server + "/assets/../viewer.py", timeout=3)
    assert error.value.code == 404


def port(device, serial_number=None, vid=0x303A, pid=0x1001):
    return SimpleNamespace(device=device, serial_number=serial_number, vid=vid, pid=pid)


def test_default_port_selection_requires_one_serial_jtag_device(history, monkeypatch):
    monkeypatch.setattr(viewer.list_ports, "comports", lambda: [
        port("esp", "11:22:33:44:55:66"),
        port("rp2040", vid=0x2E8A),
        port("kivo-runtime", "AA:BB:CC:DD:EE:FF", pid=0x4002),
    ])
    assert viewer.SerialReader(history, None, None).resolve_port() == "esp"


@pytest.mark.parametrize("serial_number", ["aabbccddeeff", "aa:bb:cc:dd:ee:ff"])
def test_explicit_serial_normalizes_case_and_colons(history, monkeypatch, serial_number):
    monkeypatch.setattr(viewer.list_ports, "comports", lambda: [
        port("first", "11:22:33:44:55:66"),
        port("selected", "AA:BB:CC:DD:EE:FF", pid=0x4002),
    ])
    assert viewer.SerialReader(history, serial_number, None).resolve_port() == "selected"


@pytest.mark.parametrize("ports", [[], [port("first"), port("second")]])
def test_missing_or_ambiguous_device_is_not_silently_selected(history, monkeypatch, ports):
    monkeypatch.setattr(viewer.list_ports, "comports", lambda: ports)
    with pytest.raises(serial.SerialException):
        viewer.SerialReader(history, None, None).resolve_port()


def test_explicit_port_bypasses_discovery(history, monkeypatch):
    def unexpected_discovery():
        raise AssertionError("explicit port must not enumerate devices")

    monkeypatch.setattr(viewer.list_ports, "comports", unexpected_discovery)
    assert viewer.SerialReader(history, None, "chosen-port").resolve_port() == "chosen-port"
