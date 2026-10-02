"""Frame-accurate measurement of a player skill: per-frame EP, player state, damage events.

measure(skill, level, hold_frames, dist, frames) -> dict with
  cost_on_press  : EP lost on the activation frame
  drain          : list of (frame, EP lost) after activation (excludes the +3% regen tick)
  hits           : [(frame, kind, sub, mult, dmg)]
  idle_frame     : first frame (after activation) the player is back in the idle state
Enemy pinned `dist` px below the player (player faces down), healed every frame; player HP refilled.
"""
import json
import sys
from common import *
from combat import setup, pin, REC
from emutools import catch
from dmglog import DMG_RET

IDLE = 0x03002B44


def measure(skill, level, hold, dist=60, frames=260, st='buu_fight_swforest.ss', maxep=999, keyfn=None, face=0,
            cost_factor=None):
    e = emu(st)
    e.w8(REC + 0xc, 1); e.w8(REC + 0xd, skill); e.w8(REC + 0x15, 0)
    e.w8(REC + 0x2e, level); e.w8(REC + 0x2f, level)
    e.w16(REC + 0x1c, maxep); e.w16(REC + 0x1a, maxep)
    e.w16(REC + 0x1e, maxep); e.w16(REC + 0x20, maxep)
    if cost_factor is not None:
        e.w8(REC + 0x56, cost_factor)
    p, en = setup(e)
    e.w8(p + 0x10, face)
    dx, dy = {0: (0, dist), 1: (0, -dist), 2: (-dist, 0), 3: (dist, 0)}[face]
    hits = []

    def cb(e, pc):
        hit = e.reg(4)
        if e.r32(hit + 0xc) != p:
            return
        ad = e.r32(hit + 8)
        hits.append((F[0], e.r8(ad), e.r8(ad + 1), e.r32(hit + 4), round(e.reg(0) / 1024, 3)))

    F = [0]
    kf = keyfn or (lambda f: KEY.B if f < hold else 0)
    ep = []
    states = []
    for f in range(frames):
        F[0] = f
        pin(e, en[0], p, dx, dy)
        e.w32(en[0] + 0x28c, 99999 << 10)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        before = e.r16(REC + 0x1a)
        catch(e, [DMG_RET], 1, lambda _f, f=f: kf(f), cb)
        after = e.r16(REC + 0x1a)
        ep.append(before - after)
        states.append(e.r32(e.r32(0x03001c34) + 0x14))
    idle = next((i for i in range(1, frames) if states[i] == IDLE and states[i - 1] != IDLE), None)
    drain = [(i, d) for i, d in enumerate(ep) if d > 0]
    return dict(skill=skill, level=level, hold=hold, dist=dist, cost_on_press=ep[0], drain=drain,
                ep_total=sum(d for d in ep if d > 0), hits=hits, idle_frame=idle, form_after=e.r8(REC + 0x14))


if __name__ == '__main__':
    sk, lv, hold = int(sys.argv[1], 0), int(sys.argv[2]), int(sys.argv[3])
    dist = int(sys.argv[4]) if len(sys.argv) > 4 else 60
    r = measure(sk, lv, hold, dist)
    print(json.dumps({k: v for k, v in r.items() if k not in ('drain', 'hits')}))
    print('drain', r['drain'][:60])
    print('hits', r['hits'][:12], '... n=', len(r['hits']))
