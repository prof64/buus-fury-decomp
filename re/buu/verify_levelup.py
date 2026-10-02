"""Verify the level-up growth formula: at LevelUp (0x08013678) entry snapshot RNG + record, predict new
max HP/EP with the Python model, then compare to what the game wrote."""
from common import *
from emutools import catch
from ents import enemies
from tables import growth
from rng import Rng

REC = 0x03001560
LEVELUP = 0x08013678
e = emu('buu_fight_swforest.ss')
e.w32(REC + 0x24, 2874)
for o in enemies(e):
    e.w32(o + 0x28c, 1 << 10)
snap = {}


def cb(e, pc):
    snap['rng'] = (e.r32(0x03004a5c), e.r32(0x03004a60))
    snap['old'] = e.reg(1); snap['new'] = e.reg(2)
    snap['maxhp'] = e.r16(REC + 0x18); snap['maxep'] = e.r16(REC + 0x1c)


catch(e, [LEVELUP], 400, lambda f: KEY.A if f % 8 < 4 else 0, cb)
print('snapshot', snap, 'after: maxHP', e.r16(REC + 0x18), 'maxEP', e.r16(REC + 0x1c))
g = growth(0)
r = Rng(*snap['rng'])
hp, ep = snap['maxhp'], snap['maxep']
for lv in range(snap['old'], snap['new']):
    a, b, c, d = r.randn(256), r.randn(256), r.randn(256), r.randn(256)
    hp = hp + ((((g[0] * a) >> 8) + g[1]) >> 16)
    hp = hp + ((hp * (((g[2] * b) >> 8) + g[3])) >> 16)
    ep = ep + (((g[4] * c) + g[5]) >> 16)
    ep = ep + ((ep * (((g[6] * d) >> 8) + g[7])) >> 16)
print('predicted maxHP', hp, 'maxEP', ep)
