"""Probe: player +0x1CC ('can be targeted by grab/special attacks') across player actions; record+0x67."""
from common import *
from bv_buu_lib import *

e, o = setup(0x1a, dx=80)
p = e.r32(PLAYER)
print('rec+0x67', e.r8(REC + 0x67), 'p+0x1cc', e.r32(p + 0x1cc), 'state obj', hex(e.r32(p + 0x14)), 'vt', hex(e.r32(e.r32(p + 0x14))))
for name, k, n in (('idle', 0, 20), ('right', KEY.RIGHT, 10), ('idle2', 0, 30), ('A', KEY.A, 3), ('after A', 0, 30),
                   ('R block', KEY.R, 10), ('released', 0, 30)):
    e.set_keys(k)
    e.run(n)
    st = e.r32(p + 0x14)
    print(f'{name:8s} p+0x1cc', e.r32(p + 0x1cc), 'ctrl', hex(st), 'vt', hex(e.r32(st)) if st else None)
