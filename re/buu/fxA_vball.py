"""fxA_vball.py -- hit an enemy with Ultra Volleyball and show which sprite def / anim / vtable it switches to."""
from common import *
from combat import setup, pin, REC
from ents import enemies

DEFS = 0x086B6BD8
defs = {u32(DEFS + 4 * i): i for i in range(298)}
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, 23); e.w8(REC + 0x15, 0)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, 0)
o = en[0]


def snap(tag):
    words = {k: e.r32(o + k) for k in range(0, 0x300, 4)}
    hits = {hex(k): defs[v] for k, v in words.items() if v in defs}
    print(tag, e.r32(0x03000f94), 'vt', hex(e.r32(o)), 'defs', hits, 'anim+50', hex(e.r32(o + 0x50)), 'dir', e.r8(o + 0x10),
          'fr', e.r8(o + 0x11), 'tick', e.r8(o + 0x12), 'state', hex(e.r32(o + 0x238)))


for f in range(400):
    if f < 30:
        pin(e, o, p, 0, 40)
    e.set_keys(KEY.B if f < 4 else 0)
    e.run(1)
    if f in (21, 22, 23, 24, 360, 361, 362, 363, 364, 365):
        snap(f)
