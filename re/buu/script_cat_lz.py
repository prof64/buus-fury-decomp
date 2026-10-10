"""script_cat_lz.py -- headerless Webfoot stream decompression as used by the script runner
(0x08049120 -> IWRAM 0x03000040: r0 = dst, r1 = stream; no type/size header). The literal width is set
by the stream (escape 'set literal width'), we start with 8. Validated against work/buu/decomp/*.bin
(emulator-run outputs)."""
import struct
import sys
from pathlib import Path
from common import *

sys.path.insert(0, str(ROOT / 'tools' / 'extract'))
import gba  # noqa: E402


def decompress_stream(data, off, litbits=8):
    fake = bytearray(struct.pack('<II', 1, 0))
    # run gba.wf_decompress on a patched copy: header (type=1) placed just before the stream
    buf = bytes(data[:off - 8]) + struct.pack('<II', 1, 0) + bytes(data[off:]) if off >= 8 else None
    out, end = gba.wf_decompress(buf, off - 8)
    return out, end


if __name__ == '__main__':
    ok = bad = 0
    for f in sorted((WORK / 'decomp').glob('*.bin')):
        src = int(f.stem, 16)
        ref = f.read_bytes()
        try:
            out, _ = decompress_stream(rom(), ra(src))
        except Exception as e:  # noqa
            out = b''
        if out[:len(ref)] == ref or ref[:len(out)] == out and len(out) > 0:
            ok += 1
        else:
            bad += 1
            print('mismatch', f.stem, len(out), len(ref), out[:16].hex(), ref[:16].hex())
    print('ok', ok, 'bad', bad)
