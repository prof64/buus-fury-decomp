"""Decompress Webfoot-compressed blobs by running the game's own decompressor (0x08049120 ->
IWRAM 0x03000040, args (dst, src)) inside the emulator. Results are cached in work/buu/decomp/."""
import sys
from pathlib import Path
from common import *
from emutools import call

CACHE = WORK / 'decomp'
CACHE.mkdir(exist_ok=True)
DST = 0x0203E000
_e = None


def _emu():
    global _e
    if _e is None:
        _e = emu('buu_snakeway_start.ss')
        _e.run(2)
    return _e


def decompress(src, maxlen=0x1000):
    f = CACHE / f'{src:08x}.bin'
    if f.exists():
        return f.read_bytes()
    e = _emu()
    st = e.save()
    for i in range(0, maxlen, 4):
        e.w32(DST + i, 0xEEEEEEEE)
    call(e, 0x08049120, (DST, src, 0, 0))
    data = e.read(DST, maxlen)
    e.load(st)
    # trim trailing fill
    end = len(data)
    while end >= 4 and data[end - 4:end] == b'\xee\xee\xee\xee':
        end -= 4
    data = data[:end]
    f.write_bytes(data)
    return data


if __name__ == '__main__':
    src = int(sys.argv[1], 16)
    d = decompress(src)
    print(len(d), d[:64].hex())
