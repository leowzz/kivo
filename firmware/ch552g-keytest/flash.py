#!/usr/bin/env python3
"""Flash this diagnostic only to the recorded CH552, preserving its EEPROM."""
from datetime import datetime
import hashlib
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
UID = '08-FC-07-BD-00-00-00-00'


def run(*args):
    result = subprocess.run(['wchisp', *map(str, args)], text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, end='', flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
    return result.stdout


def config(info):
    match = re.search(r'Current config registers: ([0-9a-fA-F]+)', info)
    if not match:
        raise SystemExit('Config response missing; refusing to flash')
    # Compare the defined low configuration bytes. Response offsets 10-11
    # changed between read-only sessions before the first erase; keep the
    # full response in evidence rather than treating those bytes as stable.
    return bytes.fromhex(match[1])[:10]


def main():
    firmware = HERE / 'build/keytest.hex'
    if not firmware.is_file():
        raise SystemExit('Build keytest.hex first')
    info = run('info')
    if f'Chip UID: {UID}' not in info or 'Chip: CH552[0x5211]' not in info:
        raise SystemExit('Wrong chip or UID; refusing to flash')
    before_config = config(info)
    backup = HERE / 'captures' / datetime.now().strftime('flash-%Y%m%d-%H%M%S')
    backup.mkdir(parents=True)
    (backup / 'info-before.txt').write_text(info)
    (backup / 'firmware.sha256').write_text(hashlib.sha256(firmware.read_bytes()).hexdigest() + '\n')
    run('eeprom', 'dump', backup / 'eeprom-before.bin')
    if (backup / 'eeprom-before.bin').stat().st_size != 128:
        raise SystemExit('EEPROM backup length mismatch; refusing to flash')
    # wchisp verifies programmed bytes by default. Stay in ISP for preservation checks.
    output = run('flash', '--no-reset', firmware)
    (backup / 'flash.log').write_text(output)
    if 'Verify OK' not in output:
        raise SystemExit('Flash did not report successful verification')
    run('eeprom', 'dump', backup / 'eeprom-after.bin')
    after = run('info')
    (backup / 'info-after.txt').write_text(after)
    if config(after) != before_config:
        raise SystemExit('Config changed unexpectedly; retained ISP for inspection')
    if (backup / 'eeprom-before.bin').read_bytes() != (backup / 'eeprom-after.bin').read_bytes():
        raise SystemExit('EEPROM changed unexpectedly; retained ISP for inspection')
    run('reset')
    print('Flash verified; EEPROM and chip config unchanged. Check runtime USB enumeration.')
    print('Evidence:', backup)


if __name__ == '__main__':
    main()
