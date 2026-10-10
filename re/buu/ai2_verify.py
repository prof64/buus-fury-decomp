"""ai2_verify.py [type ...] -- emulator checks of decoded enemy AI parameters.

For each target enemy: load/warp to an area where it spawns, then
  * aggro  : player pinned at dx px (same y) from the pinned enemy; does the enemy leave its initial
             state within 40 frames?  scan dx to find the threshold.
  * speed  : from the aggro'd state, player pinned 130 px away; per-frame enemy displacement while
             the AI state is a movement state.
  * timing : frames spent in attack states (state transitions logged from the GotoState target
             0x0800AA4A, r1 = new state id) and projectile spawns (0x0801191A).
"""
import ai2_boot  # noqa: F401
import math
import sys
from common import *
from ents import enemies, PLAYER, TYPE_TAB
from warp import warp
from ai_decode import initial_state
from emutools import call

REPLACE = 0x0800AA4A
SPAWN = 0x0801191A
REC = 0x03001560

SCENES = {
    0x44: ('state', 'buu_fight_swforest.ss'),            # Majin Fighter, SW forest (8,3)
    0x2C: ('warp', 2, 10, 100, 120),                     # Gunman, East District 439
    0x20: ('warp', 10, 8, 60, 100),                      # Ghost, Pilaf's Castle
    0x4B: ('warp', 2, 14, 100, 120),                     # Mercenary, East District 439
    # bosses gated by story flags: run their AI on the SW-forest Majin Fighter body ("transplant")
    0x91: ('transplant',),                               # Yakon
    0x6D: ('transplant',),                               # Spopovich
    0x12: ('transplant',),                               # Dabura
    0x60: ('transplant',),                               # Pui Pui
}
CHASE_DIST = {0x2C: 80, 0x4B: 70}


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def pos(e, o):
    return s32v(e.r32(o + 0x1a0)) / 256, s32v(e.r32(o + 0x1a4)) / 256


def setpos(e, o, x, y):
    e.w32(o + 0x1a0, int(x * 256) & 0xffffffff)
    e.w32(o + 0x1a4, int(y * 256) & 0xffffffff)


def etype(e, o):
    return (e.r32(o + 0x224) - TYPE_TAB) // 28


def scene(t):
    sc = SCENES[t]
    if sc[0] == 'transplant':
        e = emu('buu_fight_swforest.ss')
        e.run(30)
        obs = [o for o in enemies(e) if etype(e, o) == 0x44]
        return e, obs[0]
    if sc[0] == 'state':
        e = emu(sc[1])
    else:
        e = emu('buu_snakeway_start.ss')
        ok = warp(e, *sc[1:])
        if not ok:
            return None, None
    e.run(30)
    obs = [o for o in enemies(e) if etype(e, o) == t]
    return e, (obs[0] if obs else None)


def park_others(e, keep):
    for o in enemies(e):
        if o != keep:
            setpos(e, o, 0x7000, 0x7000)


def step(e, n, fn=None):
    """run n frames; fn(e) called before each frame; returns list of (frame, state) transitions."""
    bid = e.breakpoint(REPLACE)
    sid = e.breakpoint(SPAWN)
    trans, spawns = [], []
    for i in range(n):
        if fn:
            fn(e, i)
        e.w16(REC + 0x16, e.r16(REC + 0x18))       # keep the player alive
        e.run(1)
        for h in e.hits():
            if h['addr'] == REPLACE:
                trans.append((i, h['regs'][0], h['regs'][1]))
            elif h['addr'] == SPAWN:
                spawns.append(i)
    e.clear(bid)
    e.clear(sid)
    return trans, spawns


def aggro_scan(e, o, t, dxs):
    p = e.r32(PLAYER)
    ex, ey = pos(e, o)
    setpos(e, p, ex + 200, ey)
    call(e, REPLACE, (o + 0x238, initial_state(t), 0, 0))     # back to the idle/initial state
    base = e.save()
    res = {}
    for dx in dxs:
        e.load(base)
        park_others(e, o)

        def pin(e, i):
            setpos(e, o, ex, ey)
            setpos(e, p, ex + dx, ey)
        tr, _ = step(e, 40, pin)
        res[dx] = [(f, hex(s)) for f, ob, s in tr if ob == o + 0x238][:1]
    e.load(base)
    return res


def chase(e, o, t, dist=95, frames=900):
    p = e.r32(PLAYER)
    park_others(e, o)
    ex, ey = pos(e, o)
    track = []

    def pin(e, i):
        setpos(e, p, ex + dist, ey)
        track.append(pos(e, o))
    tr, sp = step(e, frames, pin)
    steps = [math.hypot(b[0] - a[0], b[1] - a[1]) for a, b in zip(track, track[1:])]
    return tr, sp, steps


def main(ts):
    for t in ts:
        e, o = scene(t)
        print(f'=== type {t:#x} initial state {initial_state(t):#x} obj {o and hex(o)}')
        if not o:
            print('   enemy not present')
            continue
        res = aggro_scan(e, o, t, list(range(40, 132, 4)))
        print('   aggro (dx px -> first new state):', {k: v for k, v in res.items()})
        tr, sp, steps = chase(e, o, t, CHASE_DIST.get(t, 95))
        moving = [round(s, 4) for s in steps if s > 0.01]
        hist = {}
        for s in moving:
            hist[s] = hist.get(s, 0) + 1
        mine = [(f, s) for f, ob, s in tr if ob == o + 0x238]
        print('   transitions (frame, state):', [(f, hex(s)) for f, s in mine][:40])
        dwell = {}
        for (f0, s0), (f1, _) in zip(mine, mine[1:]):
            dwell.setdefault(hex(s0), []).append(f1 - f0)
        print('   dwell frames per state:', dwell)
        print('   spawn frames:', sp[:20])
        print('   per-frame step histogram:', sorted(hist.items(), key=lambda kv: -kv[1])[:8])


if __name__ == '__main__':
    main([int(a, 0) for a in sys.argv[1:]] or sorted(SCENES))
