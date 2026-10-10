"""seg_bodybox.py SEGDIR NAME FRAME -- replay a segment to video frame FRAME; at every enemy message handler's body
box test (0x0800FB26, after 0x0800FE78) print the object (r4), its box (sp+0x6C..0x78) and the message rect"""
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402,F401
from route import start_emu  # noqa: E402

d, name, fr = Path(sys.argv[1]).resolve(), sys.argv[2], int(sys.argv[3])
lines = (d / f'{name}.json').read_text().split('\n')
keys = [json.loads(ln.rstrip(','))['keys'] for ln in lines[1:] if ln.startswith('{')]
e = start_emu(str(d / 'states' / f'{name}.ss'))
for f in range(fr + 1):
    if f == fr:
        e.breakpoint(0x0800FBF0)
        e.breakpoint(0x0800FC00)
    e.set_keys(keys[f])
    e.run(1)
    if f == fr:
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            r = h['regs']
            sp = r[13]
            box = struct.unpack('<4i', e.read(sp + 0x6C, 16))
            o = r[4]
            sv = lambda v: v - (1 << 32) if v & 0x80000000 else v  # noqa: E731
            print(hex(h["pc"] if "pc" in h else 0), hex(o), "1cc", e.r32(o + 0x1CC), "pos", sv(e.r32(o + 0x1A0)) >> 8, sv(e.r32(o + 0x1A4)) >> 8, 'z', sv(e.r32(o + 0x1A8)),
                  'box', box, 'facing', e.r8(o + 0x10))
