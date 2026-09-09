#!/usr/bin/env python3
"""Run behavioral checks and independently decode the built Intel HEX."""
import importlib.util
from pathlib import Path
import re
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent


def main():
    with tempfile.TemporaryDirectory(prefix='ch552-test-') as temp:
        root = Path(temp)
        # Keep the actual algorithm unchanged; replace only target headers/SFRs.
        source = re.sub(r'^#include[^\n]*\n', '', (HERE / 'main.c').read_text(), flags=re.M)
        (root / 'main-host.inc').write_text(source)
        subprocess.run(['clang++', '-std=c++17', '-Wall', '-Wextra', '-I' + temp,
                        str(HERE / 'test_core.cpp'), '-o', str(root / 'check')], check=True)
        subprocess.run([str(root / 'check')], check=True)
    memory = {}
    eof = False
    for line in (HERE / 'build/keytest.hex').read_text().splitlines():
        assert line.startswith(':') and not eof
        row = bytes.fromhex(line[1:])
        assert len(row) == row[0] + 5 and sum(row) % 256 == 0
        address = int.from_bytes(row[1:3], 'big')
        if row[3] == 1:
            eof = True
        else:
            assert row[3] == 0
            for i, byte in enumerate(row[4:-1]):
                assert address + i not in memory
                memory[address + i] = byte
    assert eof and max(memory) < 0x3800
    for vector in (0, 0x0B, 0x3B, 0x43):
        assert memory[vector] == 2  # LJMP reset, Timer0, USB.
        target = memory[vector + 1] * 256 + memory[vector + 2]
        assert target in memory
    binary = bytes(memory.get(i, 255) for i in range(max(memory) + 1))
    assert 'Kivo CH552 KeyTest'.encode('utf-16-le') in binary
    assert '08FC07BD-TEST'.encode('utf-16-le') in binary
    assert bytes([0x09, 0x12, 0x5c, 0xc5]) in binary
    module_spec = importlib.util.spec_from_file_location('monitor', HERE / 'monitor.py')
    module = importlib.util.module_from_spec(module_spec)
    module_spec.loader.exec_module(module)
    sample = module.parse_line('T,1000,42,65536,65536,0,0,4000,5100,5200,5300,5400,5000,5100,5200,5300,5400')
    assert sample['delta'] == [1000, 0, 0, 0, 0] and sample['keys'] == 65536
    assert module.parse_line('K,1000,17,1')['down']
    for bad in ('K,0,18,1', 'K,0,1,2', 'T,0,1', 'junk'):
        try:
            module.parse_line(bad)
        except ValueError:
            continue
        raise AssertionError('Accepted malformed input: ' + bad)
    print(f'PASS: HEX checksums, vectors, app-only range, USB identity, serial parsing; {len(memory)} mapped bytes')


if __name__ == '__main__':
    main()
