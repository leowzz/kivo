#!/usr/bin/env python3
"""Build the standalone temporary diagnostic, with a pinned CH55xduino toolchain."""
import hashlib
from pathlib import Path
import re
import subprocess
import tarfile
import urllib.request

HERE = Path(__file__).resolve().parent
CACHE = Path.home() / '.cache/kivo-ch552'
COMMIT = 'c9f9a2a6516255284064a9dd248670545f25a322'
ARCHIVE = 'sdcc-mcs51-x86_64-apple-macosx-20220422-13407_4.tar.bz2'
SHA256 = '95380334bf51cf8e29e2654b59c8840528f94dc7367d411bb66efe48d5260746'


def run(*args):
    subprocess.run([str(a) for a in args], check=True)


def main():
    CACHE.mkdir(parents=True, exist_ok=True)
    source = CACHE / 'ch55xduino'
    if not source.exists():
        run('git', 'clone', 'https://github.com/DeqingSun/ch55xduino.git', source)
        run('git', '-C', source, 'checkout', '--detach', COMMIT)
    actual = subprocess.check_output(['git', '-C', str(source), 'rev-parse', 'HEAD'], text=True).strip()
    if actual != COMMIT:
        raise SystemExit(f'Unexpected dependency revision: {actual}; expected {COMMIT}')
    archive = CACHE / ARCHIVE
    if not archive.exists():
        urllib.request.urlretrieve('https://github.com/DeqingSun/ch55xduino/releases/download/0.0.20/' + ARCHIVE, archive)
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == SHA256
    compiler = CACHE / 'toolchain/sdcc'
    if not compiler.exists():
        with tarfile.open(archive) as tar:
            tar.extractall(CACHE / 'toolchain', filter='data')
    core_root = source / 'ch55xduino/ch55x'
    core = core_root / 'cores/ch55xduino'
    output = HERE / 'build'
    output.mkdir(exist_ok=True)
    flags = ['-c', '-mmcs51', '--model-large', '--int-long-reent',
             '-Ddouble=float', '-DUSE_STDINT', '-D__PROG_TYPES_COMPAT__',
             '-DCH552', '-DF_CPU=12000000L', '-DF_EXT_OSC=0', '-DUSER_USB_RAM=266',
             '-I' + str(core), '-I' + str(core_root / 'variants/ch552')]
    sources = [HERE / 'main.c', core / 'wiring.c', *sorted((HERE / 'usb').glob('*.c'))]
    objects = []
    for path in sources:
        obj = output / (path.stem + '.rel')
        run(compiler / 'bin/sdcc', *flags, path, '-o', obj)
        # Same 2-byte area alignment as CH55xduino's sdcc.sh wrapper.
        text = obj.read_text()
        text = re.sub(r'^A (CSEG|GSINIT|GSFINAL) size ([0-9A-F]+)',
                      lambda m: f'A {m[1]} size {(int(m[2], 16) + 1) & ~1:X}',
                      text, flags=re.M)
        obj.write_text(text)
        objects.append(obj)
    run(compiler / 'bin/sdcc', '-mmcs51', '--model-large', '--int-long-reent',
        '--nostdlib', '-L' + str(compiler / 'share/sdcc/lib/large_int_calc_stack_auto'),
        '--code-size', '14336', '--xram-loc', '266', '--xram-size', '758',
        *objects, '-lmcs51', '-llibsdcc', '-lliblong', '-lliblonglong', '-llibint', '-llibfloat',
        '--out-fmt-ihx', '-o', output / 'keytest.hex')
    print((output / 'keytest.mem').read_text())
    print('SHA256', hashlib.sha256((output / 'keytest.hex').read_bytes()).hexdigest())


if __name__ == '__main__':
    main()
