"""0x29: BG layer object slot -> offset = def offset + (a, b) px (layer vtable 0x08049410, method 0x08007FE8)."""
import json
from script_v2_lib import *
import script_v2_t1 as T

S = 'buu_snakeway_start.ss'
e1, r1 = run(S, [1, 8, 4, ('n', 0x29)], 10)
e0, r0 = run(S, [1, 8, 4] + pops(3), 10)
o = e1.r32(0x03001994)
a = (e0.r32(o + 0x44), e0.r32(o + 0x48))
b = (e1.r32(o + 0x44), e1.r32(o + 0x48))
io0, io1 = e0.read(0x04000010, 16), e1.read(0x04000010, 16)
shot(e1, 'bg_offset')
T.rec(0x29, b[0] - a[0] == 8 << 10 and b[1] - a[1] == 4 << 10,
      f'set_bg_offset(layer 1, 8, 4): layer object {o:#x} (vtable 0x08049410 = tilemap layer) +0x44/+0x48 '
      f'{[hex(x) for x in a]} -> {[hex(x) for x in b]} (def offset + (8,4)<<10); BG scroll regs changed: {io0 != io1}')
T.RES.write_text(json.dumps(T.res, indent=1))
