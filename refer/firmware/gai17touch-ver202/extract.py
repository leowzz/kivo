#!/usr/bin/env python3
"""Reproduce the static extraction for the pinned gai17Touch Ver202 HEX.

Uses Python's standard library only. Does not access a USB device.
Addresses below belong to this exact firmware, not arbitrary CH552 firmware.
"""
from collections import Counter, defaultdict
from pathlib import Path
import hashlib
import json
import re

ROOT = Path(__file__).resolve().parent
EXPECTED_SHA256 = 'a8e594a2ca695191ef610fe2d3e69644f780e0143d334aa8411e8c5e2ef11834'


def yaml_lines(value, indent=0):
    """Emit JSON-quoted scalars in a YAML block structure."""
    pad = ' ' * indent
    if isinstance(value, dict):
        for key, item in value.items():
            prefix = pad + json.dumps(str(key), ensure_ascii=False) + ':'
            if isinstance(item, (dict, list)) and item:
                yield prefix
                yield from yaml_lines(item, indent + 2)
            else:
                yield prefix + ' ' + json.dumps(item, ensure_ascii=False)
    elif isinstance(value, list):
        for item in value:
            if isinstance(item, (dict, list)) and item:
                yield pad + '-'
                yield from yaml_lines(item, indent + 2)
            else:
                yield pad + '- ' + json.dumps(item, ensure_ascii=False)


def parse_hex(path):
    memory, types, base, eof = {}, Counter(), 0, False
    for number, line in enumerate(path.read_text().splitlines(), 1):
        assert line.startswith(':') and not eof, number
        row = bytes.fromhex(line[1:])
        assert len(row) == row[0] + 5 and sum(row) % 256 == 0, number
        size, address, kind = row[0], int.from_bytes(row[1:3], 'big'), row[3]
        types[kind] += 1
        if kind == 0:
            for offset, byte in enumerate(row[4:4 + size]):
                location = base + address + offset
                assert location not in memory, (number, location)
                memory[location] = byte
        elif kind in (2, 4):
            assert size == 2
            base = int.from_bytes(row[4:6], 'big') << (4 if kind == 2 else 16)
        elif kind == 1:
            assert size == 0 and address == 0
            eof = True
        else:
            raise ValueError(f'Unsupported record type {kind}')
    assert eof
    return memory, types


def hid_items(blob, base):
    names = {
        (0, 8): 'Input', (0, 9): 'Output', (0, 10): 'Collection',
        (0, 11): 'Feature', (0, 12): 'End Collection',
        (1, 0): 'Usage Page', (1, 1): 'Logical Minimum',
        (1, 2): 'Logical Maximum', (1, 3): 'Physical Minimum',
        (1, 4): 'Physical Maximum', (1, 5): 'Unit Exponent',
        (1, 6): 'Unit', (1, 7): 'Report Size', (1, 8): 'Report ID',
        (1, 9): 'Report Count', (2, 0): 'Usage',
        (2, 1): 'Usage Minimum', (2, 2): 'Usage Maximum',
    }
    offset, depth, report_id, size, count = 0, 0, 0, 0, 0
    items, bits = [], defaultdict(int)
    while offset < len(blob):
        start, prefix = offset, blob[offset]
        assert prefix != 0xFE, 'Long items not expected in this firmware'
        width, kind, tag = (0, 1, 2, 4)[prefix & 3], (prefix >> 2) & 3, prefix >> 4
        offset += 1
        payload = blob[offset:offset + width]
        assert len(payload) == width
        value = int.from_bytes(payload, 'little')
        offset += width
        name = names.get((kind, tag), f'Type {kind} tag {tag}')
        if name == 'Report Size': size = value
        elif name == 'Report Count': count = value
        elif name == 'Report ID': report_id = value
        elif name == 'Collection': depth += 1
        elif name == 'End Collection': depth -= 1
        elif name in ('Input', 'Output', 'Feature'):
            bits[(report_id, name)] += size * count
        assert depth >= 0
        items.append({'address': f'0x{base + start:04X}', 'item': name,
                      'value_hex': f'0x{value:X}',
                      'raw': blob[start:offset].hex(' ')})
    assert depth == 0
    reports = [{'report_id': rid, 'direction': direction, 'payload_bits': total,
                'wire_bytes': (total + 7) // 8 + bool(rid)}
               for (rid, direction), total in bits.items()]
    return {'items': items, 'reports': reports}


def main():
    original = ROOT / 'original.hex'
    digest = hashlib.sha256(original.read_bytes()).hexdigest()
    assert digest == EXPECTED_SHA256, 'Firmware changed: re-derive extraction addresses'
    memory, types = parse_hex(original)
    blob = bytes(memory.get(i, 0xFF) for i in range(max(memory) + 1))
    (ROOT / 'code.bin').write_bytes(blob)
    spans = []
    for address in sorted(memory):
        if not spans or address != spans[-1][1] + 1: spans.append([address, address])
        else: spans[-1][1] = address
    result = {'source_sha256': digest,
              'code_bin_sha256': hashlib.sha256(blob).hexdigest(),
              'record_counts': dict(types), 'all_checksums_valid': True,
              'mapped_bytes': len(memory), 'binary_bytes': len(blob),
              'ranges': [{'start': f'0x{a:04X}', 'end': f'0x{b:04X}',
                          'bytes': b-a+1} for a, b in spans],
              'holes_filled_with_ff': [f'0x{i:04X}' for i in range(len(blob)) if i not in memory],
              'reset_target': f'0x{int.from_bytes(blob[1:3], "big"):04X}'}
    descriptors = [('hid-input-report', 0xF9B, 200), ('hid-vendor-report', 0x1063, 34),
                   ('usb-device', 0x1085, 18), ('usb-config', 0x1097, 66)]
    result['descriptor_locations'] = []
    for name, address, length in descriptors:
        block = blob[address:address+length]
        (ROOT / f'{name}.bin').write_bytes(block)
        result['descriptor_locations'].append({'name': name, 'address': f'0x{address:04X}',
                                               'length': length, 'raw': block.hex(' ')})
        if name.startswith('hid-'):
            result[name] = hid_items(block, address)
    device = blob[0x1085:0x1097]
    assert device[:2] == bytes([18, 1])
    result['usb_device'] = {
        'usb_bcd': f'0x{int.from_bytes(device[2:4], "little"):04X}',
        'vid': f'0x{int.from_bytes(device[8:10], "little"):04X}',
        'pid': f'0x{int.from_bytes(device[10:12], "little"):04X}',
        'device_bcd': f'0x{int.from_bytes(device[12:14], "little"):04X}',
        'ep0_max_packet': device[7], 'manufacturer_index': device[14],
        'product_index': device[15], 'serial_index': device[16],
        'configuration_count': device[17]}
    config = blob[0x1097:0x10D9]
    assert int.from_bytes(config[2:4], 'little') == len(config)
    result['usb_configuration'] = {'interfaces_declared': config[4],
        'attributes': f'0x{config[7]:02X}', 'declared_max_power_ma': config[8] * 2,
        'remote_wakeup': bool(config[7] & 0x20), 'interfaces': []}
    cursor = 9
    while cursor < len(config):
        length, kind = config[cursor:cursor+2]
        assert length >= 2 and cursor + length <= len(config)
        d = config[cursor:cursor+length]
        if kind == 4:
            interface = {'number': d[2], 'class': d[5], 'subclass': d[6],
                         'protocol': d[7], 'endpoints': []}
            result['usb_configuration']['interfaces'].append(interface)
        elif kind == 0x21:
            interface['hid_bcd'] = f'0x{int.from_bytes(d[2:4], "little"):04X}'
            interface['report_descriptor_length'] = int.from_bytes(d[7:9], 'little')
        elif kind == 5:
            interface['endpoints'].append({'address': f'0x{d[2]:02X}',
                'direction': 'IN' if d[2] & 0x80 else 'OUT', 'transfer': 'interrupt',
                'max_packet': int.from_bytes(d[4:6], 'little'), 'interval_ms': d[6]})
        cursor += length
    result['strings'] = []
    for index, address in [(1, 0x10DD), (2, 0x10EB)]:
        length = blob[address]
        result['strings'].append({'index': index, 'address': f'0x{address:04X}',
            'text': blob[address+2:address+length].decode('utf-16le')})
    result['serial_generation'] = {'code_address': '0x1572', 'ram_address': '0x0162',
        'descriptor_length': 30, 'format': '14 uppercase hex digits: CHIP_ID then 6 bytes at code 0x3FFA..0x3FFF',
        'value_available_in_hex': False}
    result['ascii_candidates'] = [{'address': f'0x{m.start():04X}', 'text': m.group().decode('ascii')}
                                  for m in re.finditer(rb'[\x20-\x7E]{7,}', blob)]
    usages = {0:'None', 0x28:'Enter', 0x2A:'Backspace', 0x2B:'Tab',
        0x49:'Insert', 0x4A:'Home', 0x4B:'PageUp', 0x4C:'Delete', 0x4D:'End', 0x4E:'PageDown',
        0x4F:'Right', 0x50:'Left', 0x51:'Down', 0x52:'Up', 0x53:'NumLock',
        0x54:'KP /', 0x55:'KP *', 0x56:'KP -', 0x57:'KP +', 0x58:'KP Enter',
        0x59:'KP 1', 0x5A:'KP 2', 0x5B:'KP 3', 0x5C:'KP 4', 0x5D:'KP 5',
        0x5E:'KP 6', 0x5F:'KP 7', 0x60:'KP 8', 0x61:'KP 9', 0x62:'KP 0', 0x63:'KP .'}
    result['keymaps'] = []
    for label, address in [('primary', 0x21E5), ('alternate_a', 0x21F6), ('alternate_b', 0x2207)]:
        keys = blob[address:address+17]
        (ROOT / f'keymap-{label}.bin').write_bytes(keys)
        result['keymaps'].append({'name': label, 'address': f'0x{address:04X}',
            'entries': [{'index': i, 'usage': f'0x{code:02X}', 'key': usages[code]}
                        for i, code in enumerate(keys)]})
    result['touch_scan'] = {'table_address': '0x2551',
        'hardware_channel_selectors': list(blob[0x2551:0x2556]),
        'pins_in_order': ['P1.1', 'P1.7', 'P1.6', 'P1.5', 'P1.4']}
    result['vendor_dispatch'] = []
    for i in range(0x110D, 0x112E, 3):
        result['vendor_dispatch'].append({'opcode': f'0x{blob[i+2]:02X}',
            'handler': f'0x{int.from_bytes(blob[i:i+2], "big"):04X}'})
    (ROOT / 'extracted.yaml').write_text('\n'.join(yaml_lines(result))+'\n')
    (ROOT / 'hexdump.txt').write_text('\n'.join(
        f'{a:04X}: {blob[a:a+16].hex(" ")}' for a in range(0, len(blob), 16))+'\n')
    print(f'Validated {sum(types.values())} HEX records; extracted {len(memory)} bytes.')
    print('USB:', result['usb_device'])
    print('HID reports:', result['hid-input-report']['reports'], result['hid-vendor-report']['reports'])


if __name__ == '__main__':
    main()
