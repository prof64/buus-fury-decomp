"""Probe p+0x1cc / player controller in the plain fight state and after forcespawn without the boss nearby."""
from common import *
from ents import PLAYER
from forcespawn import spawn, setpos
from bv_buu_lib import pos

REC = 0x03001560
for label, mk in (('fight_swforest', lambda: emu('buu_fight_swforest.ss')),
                  ('forcespawn far', lambda: spawn(0x1a)[0])):
    e = mk()
    p = e.r32(PLAYER)
    if label == 'forcespawn far':
        from ents import enemies
        for o in enemies(e):
            setpos(e, o, 0x7000, 0x7000)
    for name, k, n in (('idle', 0, 30), ('right', KEY.RIGHT, 10), ('idle2', 0, 30), ('A', KEY.A, 2), ('afterA', 0, 30)):
        e.set_keys(k)
        e.run(n)
        st = e.r32(p + 0x14)
        print(label, f'{name:7s} p+0x1cc', e.r32(p + 0x1cc), 'ctrl', hex(st), 'vt', hex(e.r32(st)), 'hp', e.r16(REC + 0x16),
              'pos', pos(e, p))
