"""Emulator harness for the Buu-group boss checks.

setup(t, dx, dy)      force-spawn enemy type t (forcespawn.spawn), player placed at boss + (dx, dy) px
force(e, o, s)        replace the boss's current AI state with s (vfunc 0x0800AA4A on the AI stack obj+0x238)
trace(e, o, frames, pin=True, keys=None, damage=True)
                      run frames; returns dict with
                        states  : [(frame, new_state)]        (breakpoint 0x0800AA4A, r0 == o+0x238)
                        spawns  : [(frame, obj, vtable)]       (0x0801191A, r1 = object)
                        objs    : {obj: [(frame, words[0..0x60])]} per-frame snapshots of spawned objects
                        boss    : [(frame, x, y, anim(o+0x11), ctrl(o+0x14), ctrl_frame(o+0x12), hp)]
                        damage  : [dict(frame, target, attacker, mult, atk, atk_kind, atk_sub, final)]
                      pin=True keeps the player at its relative offset to... no: keeps the player at
                      its initial absolute position and HP topped up.
"""
from common import *
from forcespawn import spawn, setpos
from emutools import call, cur_pc
from ents import PLAYER, enemies

REPLACE = 0x0800AA4A
SPAWN = 0x0801191A
DMG = 0x0800E9D8
DMG_RET = 0x0800EAEC   # after block scaling; r4 = hit
REC = 0x03001560


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def pos(e, o):
    return s32v(e.r32(o + 0x1a0)) / 256, s32v(e.r32(o + 0x1a4)) / 256


def sw_anchor():
    from forcespawn import spawn_records, SW
    sw = [s for s in spawn_records() if (s['world'], s['map']) == SW and s['type'] == 0x44]
    return sw[0]['x'], sw[0]['y']


def setup(t, dx=60, dy=0, settle=10):
    """Spawn with the player warped in 150 px left of the boss (so the boss can't kill him during the warp),
    refill HP, then put the player at boss + (dx, dy)."""
    ax, ay = sw_anchor()
    e, o, ok = spawn(t, px=ax - 150, py=ay, settle=0)
    if not o:
        return e, None
    p = e.r32(PLAYER)
    for _ in range(settle):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.run(1)
    bx, by = pos(e, o)
    setpos(e, p, bx + dx, by + dy)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    return e, o


def force(e, o, s):
    call(e, REPLACE, (o + 0x238, s, 0, 0))


def trace(e, o, frames, pin=True, keys=None, damage=True, track_spawns=True, nwords=0x18, pinfn=None):
    p = e.r32(PLAYER)
    px, py = pos(e, p)
    res = dict(states=[], spawns=[], objs={}, boss=[], damage=[], player=[], life={})
    live = []
    hitp = [None]
    for f in range(frames):
        hp = e.r16(REC + 0x16)
        pc_ = e.r32(p + 0x14)
        res['player'].append((f, e.r16(REC + 0x18) - hp, e.r32(0x03002FE4), hex(e.r32(pc_)) if pc_ else None,
                              e.r32(p + 0x1cc)))
        if pin:
            setpos(e, p, px, py)
            e.w16(REC + 0x16, e.r16(REC + 0x18))
        if pinfn:
            pinfn(e, f)
        e.set_keys(keys(f) if keys else 0)
        st = e.save()
        bids = [e.breakpoint(a) for a in (REPLACE, SPAWN)] + ([e.breakpoint(DMG)] if damage else [])
        e.run(1)
        hs = e.hits()
        for b in bids:
            e.clear(b)
        if damage and any(h['addr'] == DMG for h in hs):
            # replay the frame step by step to read hit structs live
            e.load(st)
            e.set_keys(keys(f) if keys else 0)
            if pin:
                setpos(e, p, px, py)
            start = e.frame
            while e.frame == start:
                pc = cur_pc(e)
                if pc == DMG:
                    h = e.reg(1)
                    hitp[0] = (h, e.reg(0))
                elif pc == DMG_RET and hitp[0]:
                    h, tgt = e.reg(4), hitp[0][1]
                    atk = e.r32(h + 8)
                    res['damage'].append(dict(frame=f, target=hex(tgt), attacker=hex(e.r32(h + 0xc)),
                                              dir=e.r32(h), mult=e.r32(h + 4), atk=hex(atk),
                                              atk_kind=u8(atk) if 0x08000000 <= atk < 0x0a000000 else None,
                                              atk_sub=u8(atk + 1) if 0x08000000 <= atk < 0x0a000000 else None,
                                              crit=e.r32(h + 0x10), raw=e.r32(h + 0x14), final=e.r32(h + 0x18)))
                    hitp[0] = None
                e.step()
        for h in hs:
            if h['addr'] == REPLACE and h['regs'][0] == o + 0x238:
                res['states'].append((f, h['regs'][1]))
            elif h['addr'] == SPAWN:
                ob = h['regs'][1]
                res['spawns'].append((f, hex(ob), hex(e.r32(ob)) if ob else None))
                if track_spawns and ob:
                    live.append(ob)
                    res['objs'][ob] = []
        for ob in list(live):
            w = [e.r32(ob + 4 * k) for k in range(nwords)]
            if res['objs'][ob] and w[0] != res['objs'][ob][0][1][0]:
                live.remove(ob)          # vtable changed -> object destroyed / slot reused
                res['life'][ob] = (res['objs'][ob][0][0], f)
                continue
            res['objs'][ob].append((f, w))
        bx, by = pos(e, o)
        res['boss'].append((f, bx, by, e.r8(o + 0x11), hex(e.r32(o + 0x14)), e.r16(o + 0x12), e.r32(o + 0x28c) >> 10))
    return res


def state_runs(res):
    """[(state, start_frame, length)] from res['states']"""
    s = res['states']
    out = []
    for (f0, a), (f1, _) in zip(s, s[1:]):
        out.append((hex(a), f0, f1 - f0))
    if s:
        out.append((hex(s[-1][1]), s[-1][0], None))
    return out


def moving_fields(snaps):
    """For an object's per-frame snapshots, find word offsets that change; report first/last values."""
    if len(snaps) < 3:
        return {}
    out = {}
    nw = len(snaps[0][1])
    for k in range(nw):
        vals = [w[k] for _, w in snaps]
        if len(set(vals)) > 1:
            out[hex(4 * k)] = [s32v(v) for v in vals[:6]] + ['...'] + [s32v(v) for v in vals[-3:]]
    return out
