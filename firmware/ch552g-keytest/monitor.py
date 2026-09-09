#!/usr/bin/env python3
"""Read only the diagnostic CDC device; serve its local test dashboard."""
import argparse
from collections import deque
from datetime import datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import queue
import threading
import time

import serial
from serial.tools import list_ports

HERE = Path(__file__).resolve().parent


def parse_line(line):
    values = line.strip().split(',')
    if values[0] == 'T' and len(values) == 17:
        v = list(map(int, values[1:]))
        if not (0 <= v[2] < 1 << 17 and 0 <= v[3] < 1 << 17):
            raise ValueError('Invalid key mask')
        if any(not 0 <= n <= 65535 for n in v[6:]):
            raise ValueError('Invalid touch sample')
        return dict(kind='touch', ms=v[0], frames=v[1], keys=v[2], raw_keys=v[3],
                    calibrating=bool(v[4]), dropped=v[5], raw=v[6:11], base=v[11:16],
                    delta=[b-r for b, r in zip(v[11:16], v[6:11])])
    if values[0] == 'K' and len(values) == 4:
        ms, key_id, down = map(int, values[1:])
        if not 1 <= key_id <= 17 or down not in (0, 1):
            raise ValueError('Invalid key event')
        return dict(kind='key', ms=ms, id=key_id, down=bool(down))
    raise ValueError('Unknown diagnostic line')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', help='Exact serial port; otherwise auto-detect KeyTest')
    parser.add_argument('--http-port', type=int, default=8766)
    args = parser.parse_args()
    state = dict(connected=False, port=None, error=None, sample=None, events=[],
                 received_at=None, seen_keys=[], bad_lines=0)
    lock = threading.Lock()
    commands = queue.Queue()
    events = deque(maxlen=40)
    seen_keys = set()
    capture_dir = HERE / 'captures'
    capture_dir.mkdir(exist_ok=True)
    capture = capture_dir / (datetime.now().strftime('%Y%m%d-%H%M%S') + '.jsonl')

    def reader():
        with capture.open('a', buffering=1) as log:
            while True:
                try:
                    candidates = [p.device for p in list_ports.comports()
                                  if p.vid == 0x1209 and p.pid == 0xC55C
                                  and p.serial_number == '08FC07BD-TEST']
                    port = args.port or (candidates[0] if len(candidates) == 1 else None)
                    if port is None:
                        with lock:
                            state.update(connected=False, error='等待 CH552 KeyTest 调试串口')
                        time.sleep(1)
                        continue
                    with serial.Serial(port, 115200, timeout=0.2, write_timeout=1) as device:
                        with lock:
                            state.update(connected=True, port=port, error=None)
                        print('Connected:', port, flush=True)
                        pending = bytearray()
                        while True:
                            try:
                                command = commands.get_nowait()
                            except queue.Empty:
                                command = None
                            if command:
                                device.write(command)
                            pending.extend(device.read(device.in_waiting or 1))
                            while b'\n' in pending:
                                line, _, pending = pending.partition(b'\n')
                                try:
                                    item = parse_line(line.decode('ascii'))
                                except (ValueError, UnicodeError):
                                    with lock:
                                        state['bad_lines'] += 1
                                    continue
                                item['received_at'] = time.time()
                                log.write(json.dumps(item) + '\n')
                                with lock:
                                    state['received_at'] = item['received_at']
                                    if item['kind'] == 'touch':
                                        state['sample'] = item
                                    else:
                                        events.appendleft(item)
                                        if item['down']:
                                            seen_keys.add(item['id'])
                                        state['events'] = list(events)
                                        state['seen_keys'] = sorted(seen_keys)
                            if len(pending) > 4096:
                                pending.clear()
                except (OSError, serial.SerialException) as exc:
                    with lock:
                        state.update(connected=False, error=str(exc))
                    time.sleep(1)

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def respond(self, data, content_type='application/json', status=200):
            self.send_response(status)
            self.send_header('Content-Type', content_type)
            self.send_header('Content-Length', str(len(data)))
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            if self.path == '/':
                self.respond((HERE / 'monitor.html').read_bytes(), 'text/html; charset=utf-8')
            elif self.path == '/api/state':
                with lock:
                    data = json.dumps(state).encode()
                self.respond(data)
            else:
                self.respond(b'{}', status=404)

        def do_POST(self):
            # Only the dashboard's same-origin request can recalibrate RAM baselines.
            expected = f'http://127.0.0.1:{args.http_port}'
            if self.headers.get('Origin') != expected:
                self.respond(b'{}', status=403)
            elif self.path == '/api/calibrate':
                commands.put(b'c')
                self.respond(b'{"ok":true}')
            else:
                self.respond(b'{}', status=404)

    threading.Thread(target=reader, daemon=True).start()
    print(f'Dashboard: http://127.0.0.1:{args.http_port}\nCapture: {capture}', flush=True)
    ThreadingHTTPServer(('127.0.0.1', args.http_port), Handler).serve_forever()


if __name__ == '__main__':
    main()
