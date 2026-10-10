"""bv_others_verify.py test...  -- emulator checks for group 'others' (force-spawned on SW forest).
tests: guardian ninja ninjatp pharoah wrap worm blister enzyme pikkon"""
import sys
import json
from common import *
from bv_buu_lib import setup, force, trace, state_runs, moving_fields, pos, s32v, REC
from forcespawn import setpos, etype
from ents import PLAYER, enemies

OUT = {}
FOLLOW = [0, 8]
FACE0 = [False]


def dmg_summary(r):
    return [(d['frame'], d['target'] == hex(0) or d['target'], d['mult'], d['atk'], d['atk_kind'], d['atk_sub'],
             d['final']) for d in r['damage']]


def guardian():
    res = {}
    for dx in (int(a) for a in (sys.argv[2:] or [30, 70])):
        e, o = setup(0x3f, dx=dx, dy=0)
        force(e, o, 0x138)
        r = trace(e, o, 140)
        res[f'dx{dx}'] = dict(states=state_runs(r), damage=dmg_summary(r),
                              anim=[(f, a, c, cf) for f, x, y, a, c, cf, hp in r['boss'][:80:4]])
    return res


def ninja():
    e, o = setup(0x50, dx=90, dy=0)
    force(e, o, 0x19c)
    r = trace(e, o, 160)
    objs = {hex(k): moving_fields(v) for k, v in r['objs'].items()}
    return dict(states=state_runs(r), spawns=r['spawns'], objs=objs, life={hex(k): v for k, v in r['life'].items()},
                damage=dmg_summary(r))


def ninjatp():
    """let the ninja AI run (player 40 px away, pinned) and record boss position jumps >= 20 px"""
    e, o = setup(0x50, dx=40, dy=0)
    force(e, o, 0x199)
    r = trace(e, o, 1500, damage=False, track_spawns=False)
    jumps = []
    prev = None
    for f, x, y, a, c, cf, hp in r['boss']:
        if prev and (abs(x - prev[0]) >= 20 or abs(y - prev[1]) >= 20):
            jumps.append((f, round(x - prev[0], 1), round(y - prev[1], 1)))
        prev = (x, y)
    return dict(jumps=jumps, nstates=len(r['states']), spawns=len(r['spawns']))


def pharoah():
    e, o = setup(0x81, dx=0, dy=60)
    force(e, o, 0x2f5)
    r = trace(e, o, 60, damage=False)
    objs = {hex(k): moving_fields(v) for k, v in list(r['objs'].items())[:10]}
    return dict(states=state_runs(r)[:6], spawns=r['spawns'], objs=objs)


def wrap():
    """single shot (state 0x2f0 = align + shot + MarkTime2 + wait 24) at a player 50 px below; player NOT
    pinned in HP; log player walk/dash speed and attached wraps"""
    e, o = setup(0x81, dx=0, dy=50)
    p = e.r32(PLAYER)
    log = []
    force(e, o, 0x2f0)
    px, py = pos(e, p)
    for f in range(400):
        setpos(e, p, px, py)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        keys = 0
        if 200 <= f < 260 and f % 4 == 0:
            keys = KEY.LEFT if (f // 4) % 2 else KEY.RIGHT      # mash the d-pad
        e.set_keys(keys)
        e.run(1)
        log.append((f, e.r32(p + 0x238), e.r32(p + 0x23c)))
    ch = [log[0]] + [b for a, b in zip(log, log[1:]) if a[1:] != b[1:]]
    return dict(speed_changes=ch[:40])


def worm():
    e, o = setup(0x8e, dx=120, dy=0)
    segs = [x for x in enemies(e) if etype(e, x) == 0x8f]
    res = dict(nseg=len(segs), seg_pos0=[pos(e, s) for s in segs], head0=pos(e, o))
    track = []
    p = e.r32(PLAYER)
    px, py = pos(e, p)
    for f in range(240):
        setpos(e, p, px + 200, py)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.run(1)
        track.append([pos(e, o)] + [pos(e, s) for s in segs[:2]])
    steps = [round(abs(b[0][0] - a[0][0]) + abs(b[0][1] - a[0][1]), 3) for a, b in zip(track, track[1:])]
    res['head_step_hist'] = {str(k): steps.count(k) for k in sorted(set(steps))}
    res['track_every20'] = track[::20]
    # does seg0 reproduce the head's path with a delay?  find lag minimising distance
    # segments were parked far away by forcespawn, so compare per-frame velocities instead of positions
    vel = [[(round(b[k][0] - a[k][0], 3), round(b[k][1] - a[k][1], 3)) for k in range(3)]
           for a, b in zip(track, track[1:])]
    for k in (1, 2):
        best = None
        for lag in range(0, 40):
            d = sum(vel[i][k] != vel[i - lag][k - 1] for i in range(60, len(vel)))
            if best is None or d < best[1]:
                best = (lag, d)
        res[f'seg{k - 1}_lag_vs_prev_frames'] = best
    return res


def blister():
    e, o = setup(0x06, dx=100, dy=0)
    p = e.r32(PLAYER)
    px, py = pos(e, p)
    hist = []
    for f in range(1300):
        setpos(e, p, px + 300, py)
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.run(1)
        n = len([x for x in enemies(e) if etype(e, x) == 0x18])
        if not hist or hist[-1][1] != n:
            hist.append((f, n))
    return dict(enzyme_count_changes=hist, counter=e.r32(o + 0x204))


def enzyme():
    e, o = setup(0x18, dx=4, dy=0)
    p = e.r32(PLAYER)
    px, py = pos(e, p)
    log = []
    sts = []
    from emutools import catch  # noqa
    for f in range(400):
        setpos(e, p, px, py)
        keys = 0
        if 300 <= f < 340 and f % 4 == 0:
            keys = KEY.A if (f // 4) % 2 else KEY.B
        e.set_keys(keys)
        bid = e.breakpoint(0x0800AA4A)
        e.run(1)
        for h in e.hits():
            if h['regs'][0] == o + 0x238:
                sts.append((f, hex(h['regs'][1])))
        e.clear(bid)
        log.append((f, e.r16(REC + 0x16), e.r16(REC + 0x1a), e.r32(0x03000f94) & 63))
    ch = [log[0]] + [b for a, b in zip(log, log[1:]) if a[1:3] != b[1:3]]
    return dict(states=sts, hp_ep_changes=ch[:40])


def bloodcell():
    res = []
    for trial in range(4):
        e, o = setup(0x07, dx=150, dy=0, settle=10 + 7 * trial)
        r = trace(e, o, 200, pin=True)
        xs = [(x, y) for f, x, y, a, c, cf, hp in r['boss']]
        steps = sorted({(round(b[0] - a[0], 3), round(b[1] - a[1], 3)) for a, b in zip(xs, xs[1:])})
        res.append(dict(steps=steps[:6], states=state_runs(r)[:3]))
    e, o = setup(0x07, dx=0, dy=0)
    p = e.r32(PLAYER)

    def follow(e, f):          # keep the player on top of the cell
        bx, by = pos(e, o)
        setpos(e, p, bx + FOLLOW[0], by + FOLLOW[1])
        if FACE0[0]:
            e.w8(o + 0x10, 0)
    r = trace(e, o, 120, pin=False, pinfn=follow)
    dm = [d['frame'] for d in r['damage']]
    return dict(motion=res, damage=dmg_summary(r)[:4], damage_frames=dm[:12])


def pikkon():
    e, o = setup(0x5b, dx=24, dy=0)
    force(e, o, 0x1c4)
    r = trace(e, o, 120)
    return dict(states=state_runs(r)[:6], spawns=r['spawns'][:10], damage=dmg_summary(r)[:20],
                objs={hex(k): moving_fields(v) for k, v in list(r['objs'].items())[:3]})


if __name__ == '__main__':
    for t in [a for a in sys.argv[1:] if not a.isdigit()]:
        OUT[t] = globals()[t]()
        print('=====', t)
        print(json.dumps(OUT[t], default=str)[:6000])
    p = WORK / 'bv_others_verify.json'
    old = json.loads(p.read_text()) if p.exists() else {}
    old.update(json.loads(json.dumps(OUT, default=str)))
    p.write_text(json.dumps(old, indent=1))
