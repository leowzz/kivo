#!/usr/bin/env python3
"""Local serial-to-browser history viewer for the BLE RSSI demo."""

from __future__ import annotations

import argparse
from collections import deque
import csv
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import math
from pathlib import Path
import re
import threading
import time
from urllib.parse import parse_qs, urlsplit

import serial
from serial.tools import list_ports


RSSI_RE = re.compile(r"\brssi_dbm=(-?\d+)(?:\s|$)")
PEER_RE = re.compile(r"\b(?:peer|mac)=((?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2})(?:\s|$)")
LINK_RE = re.compile(r"^(?:LINK|STATUS)\s+state=(CONNECTED|DISCONNECTED|ADVERTISING)(?:\s|$)")
HISTORY_TYPES = {"sample", "link", "serial", "marker"}


class History:
    def __init__(self, path: Path, limit: int = 86400):
        self.path = path
        self.lock = threading.RLock()
        self.records = deque(maxlen=limit)
        self.cursor = 0
        self.state = {
            "serial_connected": False,
            "ble_state": "UNKNOWN",
            "peer": None,
            "last_sample_ts": None,
            "last_rssi": None,
            "port": None,
            "error": None,
        }
        self._serial_recorded = None
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists():
            with path.open(encoding="utf-8", errors="replace") as saved:
                for line in saved:
                    try:
                        record = json.loads(line)
                        valid = (
                            isinstance(record, dict)
                            and isinstance(record.get("id"), int)
                            and record["id"] > self.cursor
                            and record.get("type") in HISTORY_TYPES
                            and isinstance(record.get("ts"), (int, float))
                            and math.isfinite(record["ts"])
                        )
                        if not valid:
                            continue
                        if record["type"] == "sample":
                            rssi = record.get("rssi")
                            if not isinstance(rssi, int) or not -127 <= rssi <= 20:
                                continue
                            self.state.update(last_sample_ts=record["ts"], last_rssi=rssi,
                                              peer=record.get("peer"))
                        self.cursor = record["id"]
                        self.records.append(record)
                    except (ValueError, TypeError, KeyError):
                        continue
        needs_newline = False
        if path.exists() and path.stat().st_size:
            with path.open("rb") as saved:
                saved.seek(-1, io.SEEK_END)
                needs_newline = saved.read(1) != b"\n"
        self.file = path.open("a", encoding="utf-8", buffering=1)
        if needs_newline:
            self.file.write("\n")

    def _append(self, kind: str, **fields) -> dict:
        self.cursor += 1
        record = {"id": self.cursor, "type": kind, "ts": time.time(), **fields}
        self.file.write(json.dumps(record, ensure_ascii=False, allow_nan=False) + "\n")
        self.records.append(record)
        return record

    def serial_state(self, connected: bool, port: str | None = None,
                     error: str | None = None) -> None:
        with self.lock:
            changed = connected != self._serial_recorded
            self.state.update(serial_connected=connected, port=port, error=error)
            if not connected:
                self.state["ble_state"] = "UNKNOWN"
            if changed:
                self._serial_recorded = connected
                self._append("serial", state="CONNECTED" if connected else "DISCONNECTED")

    def _link(self, state: str, peer: str | None = None) -> None:
        peer_changed = peer is not None and peer != self.state["peer"]
        if self.state["ble_state"] != state or peer_changed:
            self.state["ble_state"] = state
            if peer is not None:
                self.state["peer"] = peer
            self._append("link", state=state, peer=self.state["peer"])

    def ingest(self, line: str) -> None:
        with self.lock:
            if line.startswith("RSSI "):
                strength = RSSI_RE.search(line)
                address = PEER_RE.search(line)
                if not strength or not address:
                    return
                rssi = int(strength[1])
                if not -127 <= rssi <= 20:
                    return
                peer = address[1].upper()
                self._link("CONNECTED", peer)
                record = self._append("sample", rssi=rssi, peer=peer)
                self.state.update(last_sample_ts=record["ts"], last_rssi=rssi, error=None)
                return
            match = LINK_RE.match(line)
            if match:
                address = PEER_RE.search(line)
                self._link(match[1], address[1].upper() if address else None)
            elif line.startswith(("RSSI_UNAVAILABLE", "RSSI_REQUEST_FAILED", "RSSI_TIMEOUT")):
                self.state["error"] = "蓝牙已连接，暂时无法读取信号"

    def marker(self, label: str) -> dict:
        with self.lock:
            return self._append("marker", label=label.strip()[:64] or "位置标记")

    def snapshot(self, after: int) -> dict:
        with self.lock:
            return {
                "cursor": self.cursor,
                "records": [dict(r) for r in self.records if r["id"] > after],
                "state": {**self.state, "now": time.time()},
            }

    def csv_bytes(self) -> bytes:
        with self.lock:
            records = list(self.records)
        output = io.StringIO(newline="")
        writer = csv.writer(output)
        writer.writerow(["time", "type", "rssi_dbm", "peer", "state", "label"])
        for record in records:
            label = str(record.get("label", ""))
            if label.lstrip().startswith(("=", "+", "-", "@")):
                label = "'" + label
            writer.writerow([
                datetime.fromtimestamp(record["ts"], timezone.utc).isoformat(timespec="milliseconds"),
                record["type"], record.get("rssi", ""), record.get("peer", ""),
                record.get("state", ""), label,
            ])
        return output.getvalue().encode("utf-8-sig")

    def close(self) -> None:
        with self.lock:
            self.file.close()


class SerialReader(threading.Thread):
    def __init__(self, history: History, serial_number: str | None,
                 explicit_port: str | None):
        super().__init__(name="rssi-serial", daemon=True)
        self.history = history
        self.serial_number = serial_number.replace(":", "").casefold() if serial_number else None
        self.explicit_port = explicit_port
        self.stopped = threading.Event()
        self.connection = None

    def resolve_port(self) -> str:
        if self.explicit_port:
            return self.explicit_port
        matches = []
        for port in list_ports.comports():
            if port.vid != 0x303A:
                continue
            if self.serial_number is None:
                if port.pid != 0x1001:
                    continue
            elif (port.serial_number or "").replace(":", "").casefold() != self.serial_number:
                continue
            matches.append(port.device)
        if not matches:
            raise serial.SerialException("未找到 ESP32-S3，等待重新连接")
        if len(matches) != 1:
            raise serial.SerialException("找到多个匹配串口，请指定 --serial 或 --serial-port")
        return matches[0]

    def run(self) -> None:
        while not self.stopped.is_set():
            try:
                port_name = self.resolve_port()
                connection = serial.Serial(port=None, baudrate=115200, timeout=0.5, exclusive=True)
                self.connection = connection
                connection.dtr = True
                connection.rts = False
                connection.port = port_name
                connection.open()
                connection.reset_input_buffer()
                self.history.serial_state(True, port_name)
                while not self.stopped.is_set():
                    raw = connection.readline(2048)
                    if not raw:
                        continue
                    if not raw.endswith(b"\n"):
                        # Do not parse a truncated or oversized serial record.
                        continue
                    self.history.ingest(raw.decode("utf-8", errors="replace").strip())
            except (serial.SerialException, OSError) as error:
                if not self.stopped.is_set():
                    self.history.serial_state(False, error=str(error))
            finally:
                if self.connection is not None:
                    self.connection.close()
                    self.connection = None
            self.stopped.wait(1)

    def stop(self) -> None:
        self.stopped.set()
        connection = self.connection
        if connection is not None:
            try:
                connection.cancel_read()
            except (serial.SerialException, OSError):
                pass
        self.join(timeout=5)
        if self.is_alive() and connection is not None:
            connection.close()
            self.join(timeout=1)


def make_handler(history: History):
    web_directory = Path(__file__).with_name("web")
    static_files = {
        "/": ("index.html", "text/html; charset=utf-8"),
        "/viewer.html": ("index.html", "text/html; charset=utf-8"),
        "/assets/app.js": ("app.js", "text/javascript; charset=utf-8"),
        "/assets/styles.css": ("styles.css", "text/css; charset=utf-8"),
    }

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, _format, *args):
            pass

        def send_bytes(self, body: bytes, content_type: str, status: int = 200,
                       attachment: bool = False) -> None:
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.send_header("Cache-Control", "no-store")
            if attachment:
                self.send_header("Content-Disposition", 'attachment; filename="rssi-history.csv"')
            self.end_headers()
            try:
                self.wfile.write(body)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def send_json(self, data, status: int = 200) -> None:
            self.send_bytes(json.dumps(data, ensure_ascii=False, allow_nan=False).encode(),
                            "application/json; charset=utf-8", status)

        def do_GET(self):
            url = urlsplit(self.path)
            if url.path in static_files:
                filename, content_type = static_files[url.path]
                try:
                    body = (web_directory / filename).read_bytes()
                except OSError:
                    self.send_json({"error": "网页资源不存在"}, 503)
                    return
                self.send_bytes(body, content_type)
            elif url.path == "/api/state":
                try:
                    after = int(parse_qs(url.query).get("after", ["0"])[0])
                    if after < 0:
                        raise ValueError
                except (ValueError, IndexError):
                    self.send_json({"error": "after 必须是非负整数"}, 400)
                    return
                self.send_json(history.snapshot(after))
            elif url.path == "/api/export.csv":
                self.send_bytes(history.csv_bytes(), "text/csv; charset=utf-8", attachment=True)
            elif url.path == "/favicon.ico":
                self.send_bytes(b"", "image/x-icon", 204)
            else:
                self.send_json({"error": "未找到页面"}, 404)

        def do_POST(self):
            if urlsplit(self.path).path != "/api/marker":
                self.send_json({"error": "未找到接口"}, 404)
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if not 0 < length <= 4096:
                    raise ValueError
                data = json.loads(self.rfile.read(length))
                if not isinstance(data, dict) or not isinstance(data.get("label", ""), str):
                    raise ValueError
            except (ValueError, TypeError, UnicodeDecodeError):
                self.send_json({"error": "请输入有效的位置标记"}, 400)
                return
            self.send_json(history.marker(data.get("label", "")), 201)

    return Handler


def main() -> None:
    parser = argparse.ArgumentParser(description="本地 BLE 信号历史网页")
    parser.add_argument("--serial", help="ESP32-S3 USB 序列号；未指定时选择唯一的 USB Serial/JTAG 设备")
    parser.add_argument("--serial-port", help="显式指定串口，替代序列号自动定位")
    parser.add_argument("--http-port", type=int, default=8765)
    parser.add_argument("--data-file", type=Path,
                        default=Path(__file__).resolve().parents[2] / "output/ble-rssi/history.jsonl")
    args = parser.parse_args()
    history = History(args.data_file.expanduser())
    try:
        server = ThreadingHTTPServer(("127.0.0.1", args.http_port), make_handler(history))
    except OSError:
        history.close()
        raise
    reader = SerialReader(history, args.serial, args.serial_port)
    reader.start()
    print(f"RSSI 网页：http://127.0.0.1:{server.server_port}", flush=True)
    target = args.serial_port or args.serial or "自动选择唯一设备"
    print(f"ESP32-S3：{target}；历史：{history.path}", flush=True)
    try:
        server.serve_forever(poll_interval=0.25)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        reader.stop()
        history.close()


if __name__ == "__main__":
    main()
