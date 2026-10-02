"""ea_rec.py type scenario [frames] -- record how an enemy animates and what it plays, frame by frame.

Force-spawns enemy type `type` (forcespawn.spawn) and runs a scenario:
  far     player parked 110 px to the left, idle (aggro / chase / idle fidgets)
  near    player parked 36 px to the left, idle, HP pinned (attacks)
  punch   player next to the enemy, facing it, presses A every 24 frames (hurt / knockback / knockdown)
  kill    as punch, but the enemy's HP is set to 1 hit's worth first (death)
  ko      as near, but the enemy HP is set to 0 + a hit is sent: the death path only

Per frame, for every enemy object in the enemy list (0x03001BB0):
  raw (def+0x50 index == sprites.json raw_index), frame obj+0x11, tick obj+0x12, dir obj+0x10, state object obj+0x14
  (address + vtable), AI state on top of the AI stack (obj+0x238: count at +4, entries {node, def} at +8),
  x / y, HP (obj+0x28C >> 10), height obj+0x176, flags obj+0x1CC
Events (breakpoints, regs at entry):
  SetAnim(id)       0x0800ECF0  (vfunc +0x28)  r0 obj, r1 id
  SetAnimRaw(n)     0x0800ECDA                 r0 obj, r1 n (raw = n + 1), state untouched
  SetState(st)      0x0800ED18                 r0 obj, r1 state object
  SetAnimHold(id,f) 0x0800ED3E  (vfunc +0x30)  r0 obj, r1 id, r2 frame (state = hold 0x03001B00)
  SetStateId(id)    0x0800FAB6  (vfunc +0x2C)  r0 obj, r1 id (state only: table 0x080726B8)
  PlaySfx           0x0803E378                 r1 entry -> id, r2 volume, r3 pan
  StopSfx           0x0803E302                 r1 entry
each with the caller (lr - 1 when thumb)."""
import json
import sys
from common import *
from forcespawn import spawn, setpos
from ents import PLAYER, enemies, TYPE_TAB

REC = 0x03001560
SFX_TABLE = 0x083192A8
STATE_T = 0x0870F514
EV = {0x0800ECF0: 'anim', 0x0800ECDA: 'raw', 0x0800ED18: 'state', 0x0800ED3E: 'hold', 0x0800FAB6: 'stateid',
      0x0803E378: 'sfx', 0x0803E302: 'stopsfx', 0x0803E2EC: 'stopall', 0x0801191A: 'spawn'}
OUT = WORK / 'eanim'
OUT.mkdir(parents=True, exist_ok=True)


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def anim_raw(e, o):
    d = e.r32(o + 0x58)
    a = e.r32(o + 0x50)
    if not (0x08000000 <= d < 0x0A000000):
        return -1
    for r in range(e.r8(d) + 2):
        if e.r32(d + 0x50 + 4 * r) == a:
            return r
    return -1


def ai_state(e, o):
    n = e.r32(o + 0x23C)
    if not 0 < n < 16:
        return None
    d = e.r32(o + 0x238 + 8 * n + 4)
    if STATE_T <= d < STATE_T + 8 * 2000 and (d - STATE_T) % 8 == 0:
        return (d - STATE_T) // 8
    return None


def etype(e, o):
    return (e.r32(o + 0x224) - TYPE_TAB) // 28


def snap(e, o):
    st = e.r32(o + 0x14)
    return [anim_raw(e, o), e.r8(o + 0x11), e.r16(o + 0x12), e.r8(o + 0x10),
            st, e.r32(st) if 0x02000000 <= st < 0x04000000 else 0, ai_state(e, o),
            s32v(e.r32(o + 0x1a0)), s32v(e.r32(o + 0x1a4)), e.r32(o + 0x28c) >> 10,
            s32v(e.r32(o + 0x174)) if False else e.r16(o + 0x176), e.r32(o + 0x1cc)]


FIELDS = ['raw', 'frame', 'tick', 'dir', 'st', 'stvt', 'ai', 'x', 'y', 'hp', 'z', 'hittable']


def record(t, scn, frames=600):
    e, o, ok = spawn(t, settle=0)
    if not o:
        return dict(type=t, scenario=scn, error='no spawn')
    p = e.r32(PLAYER)
    bx, by = s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8
    dx = {'far': -110, 'near': -36}.get(scn, -22)
    px, py = bx + dx, by
    setpos(e, p, px, py)
    e.w8(p + 0x10, 3)
    if scn in ('kill',):
        e.w32(o + 0x28c, 1 << 10)
    ids = [e.breakpoint(a) for a in EV]
    objs = {}
    events = []
    for f in range(frames):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        if scn in ('far', 'near'):
            setpos(e, p, px, py)
        keys = 0
        if scn in ('punch', 'kill'):
            # stay next to the enemy, facing it
            ex, ey = s32v(e.r32(o + 0x1a0)) >> 8, s32v(e.r32(o + 0x1a4)) >> 8
            if ex or ey:
                setpos(e, p, ex - 20, ey)
            e.w8(p + 0x10, 3)
            keys = KEY.A if f % 24 < 2 else 0
        e.set_keys(keys)
        e.run(1)
        for h in e.hits():
            if h['kind'] != 'break':
                continue
            a = h['addr'] & ~1
            r = h['regs']
            k = EV.get(a)
            lr = (r[14] & ~1)
            if k in ('anim', 'raw', 'state', 'hold', 'stateid'):
                ev = [f, k, r[0], r[1], lr]
                if k == 'hold':
                    ev.append(r[2])
                if k == 'state':
                    ev.append(e.r32(r[1]) if 0x02000000 <= r[1] < 0x04000000 else 0)
                events.append(ev)
            elif k == 'sfx':
                events.append([f, 'sfx', 0, (r[1] - SFX_TABLE) // 12, lr, r[2], s32v(r[3])])
            elif k == 'stopsfx':
                events.append([f, 'stopsfx', 0, (r[1] - SFX_TABLE) // 12, lr])
            elif k == 'stopall':
                events.append([f, 'stopall', 0, 0, lr])
            elif k == 'spawn':
                events.append([f, 'spawn', r[1], e.r32(r[1]) if 0x02000000 <= r[1] < 0x04000000 else 0, lr])
        for q in enemies(e):
            d = objs.setdefault(q, {'type': etype(e, q), 'first': f, 'gfx': e.r16(q + 0x0e) if False else None,
                                    'def': e.r32(q + 0x58), 'rows': []})
            d['rows'].append([f] + snap(e, q))
        if not enemies(e) and f > 60:
            break
    for i in ids:
        e.clear(i)
    return dict(type=t, scenario=scn, player=[px, py], fields=['f'] + FIELDS,
                objs={'%08x' % k: v for k, v in objs.items()}, events=events)


if __name__ == '__main__':
    t = int(sys.argv[1], 0)
    scn = sys.argv[2]
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 600
    res = record(t, scn, n)
    path = OUT / ('%02x_%s.json' % (t, scn))
    path.write_text(json.dumps(res))
    print('wrote', path, 'objs', len(res.get('objs', {})), 'events', len(res.get('events', [])))
