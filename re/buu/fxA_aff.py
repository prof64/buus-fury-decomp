"""fxA_aff.py skill hold frames face [vtable] -- per frame: affine / matched fx OBJs with OAM attrs and matrices,
plus the +0x18 / +0x10 / +0x34 fields of objects with the given vtable."""
import sys
from common import *
from combat import setup, pin, REC
from fxcap import FxIndex, capture, affine
from bv_fighters_lib import world_objs

sk, hold, frames, face = (int(x, 0) for x in sys.argv[1:5])
vt = int(sys.argv[5], 16) if len(sys.argv) > 5 else None
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, face)
ix = FxIndex()
s = lambda v: v - (1 << 32) if v & 0x80000000 else v
dx, dy = {3: (200, 0), 2: (-200, 0), 0: (0, 200), 1: (0, -200)}[face]
for f in range(frames):
    if en:
        pin(e, en[0], p, dx, dy)
        e.w32(en[0] + 0x28c, 9999 << 10)
    e.w16(REC + 0x1a, 999)
    e.set_keys(KEY.B if f < hold else 0)
    e.run(1)
    px, py = s(e.r32(p + 0x1a0)) >> 8, s(e.r32(p + 0x1a4)) >> 8
    cx, cy = s(e.r32(0x030019BC)), s(e.r32(0x030019C0))
    out = []
    for o in capture(e, ix, unknown=False):
        if 'fx' in o or o['affine']:
            t = f"{o['fx'][0][0][3:] + '#' + str(o['fx'][0][1]) if 'fx' in o else 'obj' + str(o['W']) + 'x' + str(o['H'])}" \
                f"@scr{o['x'] + cx - px},{o['y'] + cy - py}"
            if o['affine']:
                t += f" AFF{'D' if o['dbl'] else ''}{affine(e, o['affine_idx'])}"
            if o['hflip'] or o['vflip']:
                t += f" {'H' if o['hflip'] else ''}{'V' if o['vflip'] else ''}"
            out.append(t)
    objs = ''
    if vt:
        objs = ' '.join(f"[st{e.r8(o + 0x10)} 18={e.r32(o + 0x18):x} 34={e.r32(o + 0x34):x}]" for o in world_objs(e) if e.r32(o) == vt)
    if out or objs:
        print(f, e.r32(0x03000f94) % 1000, objs, ' | '.join(out))
