"""Shared emulator logger for the fighters-group boss checks.

fight(t, frames, ...) force-spawns enemy type t next to Goku (forcespawn.spawn), optionally forces an AI
state / HP fraction / player distance, keeps the player alive, and logs:
  states  : (frame, state)            -- enemy replace-state 0x0800AA4A (r0 = obj, r1 = state) and push 0x0800AA9C
  spawns  : (frame, obj, vtable)      -- 0x0801191A(world, obj)
  hits    : dict(frame, target, attacker, mult, atype, sub, kind) -- ComputeDamage 0x0800E9D8(target, hit)
  tracks  : obj -> list of (frame, x, y) for spawned objects while they stay in the world lists
"""
from common import *
from emutools import catch, call
from forcespawn import spawn, setpos
from ents import PLAYER

REPLACE, PUSH, ADD, DMG = 0x0800AA4A, 0x0800AA9C, 0x0801191A, 0x0800E9D8
WORLD = 0x03001c4c
REC = 0x03001560


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def pos(e, o):
    return s32v(e.r32(o + 0x1a0)) / 256, s32v(e.r32(o + 0x1a4)) / 256


def world_objs(e):
    out = []
    n = e.r32(WORLD + 4)
    for i in range(min(n, 200)):
        out.append(e.r32(WORLD + 8 + 4 * i))
    base = WORLD + 0x40c
    n = e.r32(base)
    for i in range(min(n, 200)):
        out.append(e.r32(base + 4 + 4 * i))
    return out


def set_state(e, o, state):
    call(e, REPLACE, (o + 0x238, state, 0, 0))


def hp(e, o):
    return e.r32(o + 0x28c) / 1024


class Log:
    def __init__(self):
        self.states, self.spawns, self.hits, self.tracks = [], [], [], {}


def spawn2(t, donor=None, gap=100):
    """forcespawn.spawn variant: player HP set to 9999 before the warp (bosses kill Goku during the warp
    settle otherwise) and the boss record moved `gap` px right of the player's arrival point."""
    from forcespawn import spawn_records, own_record, SW, etype
    from warp import warp
    from ents import enemies
    e = emu('buu_snakeway_start.ss')
    e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
    sw = [s for s in spawn_records() if (s['world'], s['map']) == SW and s['type'] == 0x44]
    tgt = int(sw[0]['record'], 16)
    src = own_record(donor if donor is not None else t)
    if src is not None:
        for off in (0x10, 0x14, 0x18, 0x1c):
            e.w32(tgt + off, u32(src + off))
    e.w32(tgt + 8, t)
    for s in sw[1:]:
        r = int(s['record'], 16)
        e.w16(r + 0xc, 0x7000); e.w16(r + 0xe, 0x7000)
    ex, ey = s16(tgt + 0xc), s16(tgt + 0xe)
    e.w16(tgt + 0xc, ex + gap)
    ok = warp(e, SW[0], SW[1], ex, ey, frames=400)
    obs = [ob for ob in enemies(e) if etype(e, ob) == t]
    for ob in enemies(e):
        if ob not in obs:
            setpos(e, ob, 0x7000, 0x7000)
    return e, (obs[0] if obs else None), ok


def fight(t, frames=600, dist=None, dy=0, state=None, hp_frac=None, god=True, keys=lambda f: 0, setup=None,
          track=True, e_o=None, track_frames=200, donor=None, watch=(), every=None):
    if e_o is None:
        e, o, ok = spawn2(t, donor=donor)
    else:
        e, o = e_o
    assert o, 'spawn failed'
    p = e.r32(PLAYER)
    if dist is not None:
        bx, by = pos(e, o)
        setpos(e, p, bx - dist, by + dy)
    if hp_frac is not None:
        e.w32(o + 0x28c, int(e.r32(0x08711274 + 28 * t + 4) * hp_frac) << 10)
    if setup:
        setup(e, o)
    if state is not None:
        set_state(e, o, state)
    L = Log()
    f0 = e.frame
    live = {}

    def cb(e, pc):
        fr = e.frame - f0
        if pc in (REPLACE, PUSH) and e.reg(0) == o + 0x238:
            L.states.append((fr, e.reg(1), 'push' if pc == PUSH else 'goto'))
        elif pc == ADD:
            ob = e.reg(1)
            L.spawns.append((fr, ob, e.r32(ob)))
            if track:
                live[ob] = fr
                L.tracks[ob] = []
        elif pc == DMG:
            h = e.reg(1)
            at = e.r32(h + 8)
            L.hits.append(dict(frame=fr, target=e.reg(0), attacker=e.r32(h + 0xc), dir=e.r32(h), mult=e.r32(h + 4),
                               atype=at, kind=e.r8(at) if at else None, sub=e.r8(at + 1) if at else None,
                               power=e.r32(at + 4) if at else None))

    L.watch = []
    last = None
    for f in range(frames):
        if god:
            e.w16(REC + 0x16, 9999)
        if every:
            every(e, o, f)
        catch(e, [REPLACE, PUSH, ADD, DMG], 1, lambda _: keys(f), cb)
        if watch:
            cur = tuple(fn(e, o) for fn in watch)
            if cur != last:
                L.watch.append((e.frame - f0, cur))
                last = cur
        if track and live:
            wo = set(world_objs(e))
            for ob in list(live):
                if ob in wo and e.frame - f0 - live[ob] < track_frames:
                    L.tracks[ob].append((e.frame - f0, s32v(e.r32(ob + 0x20)) / 256, s32v(e.r32(ob + 0x24)) / 256))
                else:
                    if ob not in wo:
                        L.tracks[ob].append((e.frame - f0, 'gone'))
                    del live[ob]
    return e, o, L


def summarize(L, o, p=None):
    out = []
    out.append('states: ' + ' '.join(f'{f}:{s:#x}{"P" if k == "push" else ""}' for f, s, k in L.states))
    for f, ob, vt in L.spawns:
        tr = L.tracks.get(ob, [])
        pts = [x for x in tr if x[1] != 'gone']
        gone = [x[0] for x in tr if x[1] == 'gone']
        sp = ''
        if len(pts) >= 3:
            (f1, x1, y1), (f2, x2, y2) = pts[1], pts[min(len(pts) - 1, 6)]
            if f2 > f1:
                sp = f'v=({(x2 - x1) / (f2 - f1):.3f},{(y2 - y1) / (f2 - f1):.3f})px/f'
        out.append(f'spawn f{f} obj {ob:#x} vt {vt:#x} first {pts[0][1:] if pts else None} tracked {len(pts)}f '
                   f'{sp} gone@{gone[0] if gone else "-"}')
    for h in L.hits:
        who = 'boss->player' if h['attacker'] == o else ('player->boss' if h['target'] == o else 'other')
        out.append(f"hit f{h['frame']} {who} attacker {h['attacker']:#x} mult {h['mult']} atype {h['atype']:#x} "
                   f"kind {h['kind']} sub {h['sub']} power {h['power']}")
    return '\n'.join(out)
