"""ea_probe.py type [frames] -- force-spawn one enemy and print its vtable slots + per-frame anim fields."""
import sys
from common import *
from forcespawn import spawn

t = int(sys.argv[1], 0)
e, o, ok = spawn(t)
print('obj', hex(o), 'vt', hex(e.r32(o)))
vt = e.r32(o)
for s in range(0, 0x80, 4):
    print('  slot +%02x -> %08x' % (s, (vt + e.r32(vt + s)) & 0xffffffff))


def raw(o):
    d = e.r32(o + 0x58)
    a = e.r32(o + 0x50)
    for r in range(e.r8(d) + 2):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


for f in range(int(sys.argv[2]) if len(sys.argv) > 2 else 120):
    e.run(1)
    st = e.r32(o + 0x14)
    print(f, 'raw', raw(o), 'dir', e.r8(o + 0x10), 'fr', e.r8(o + 0x11), 'tick', e.r16(o + 0x12), 'st', hex(st),
          'stvt', hex(e.r32(st)) if st else 0, 'sh', e.r8(st + 4) if st else 0, 'x', e.r32(o + 0x1a0) >> 8)
