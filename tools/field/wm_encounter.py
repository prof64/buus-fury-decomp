"""wm_encounter.py -- record a world-map encounter (fork E): data/field/worldmap_encounter.json.

Pass 1: the rec_worldmap setup (8-1 capsule -> 0-50), then a steering controller flies the flyer into the nearest
airborne encounter (type 141, z > 0) matching its altitude; the keys of every recorded frame are logged.
Pass 2: rec_worldmap.run with those inputs (deterministic), plus per frame `enc` = the enemy list in order
[x 24.8, y 24.8, z 22.10, AI top state, controller vt] and `hits` = encounter-related calls of the frame
(IfPlayerOnTop cond 0x08036EA4 returning true, TriggerSelf 0x08036E7A / 0x08036E8E, enemy vfunc+0x74 0x0800F866)."""
import json
import math
import sys
from wm_lib import *  # noqa
from common import ROOT
import common_field as cf
import rec_worldmap as rw

N = 700
BPS = {0x08036E7A: 'trigger_self_tick', 0x08036E8E: 'trigger_self_b', 0x0800F866: 'enemy_vfunc74',
       0x080342CA: 'onTop_enter_child', 0x080342F0: 'onTop_exit_child'}


def enc(e):
    out = []
    n = e.r32(ENEMY_LIST)
    for k in range(min(n, 64)):
        o = e.r32(ENEMY_LIST + 4 + 4 * k)
        d = cf.enemy_detail(e)[k] if False else None
        st = o + 0x238
        depth = e.r32(st + 4)
        top = None
        if depth:
            sdef = e.r32(st + 12 + 8 * (depth - 1))
            top = (sdef - cf.AI_STATES) // 8
        ctl = e.r32(o + 0x14)
        out.append([s32v(e.r32(o + 0x1A0)), s32v(e.r32(o + 0x1A4)), s32v(e.r32(o + 0x1A8)), top,
                    '%08X' % (e.r32(ctl) if ctl else 0)])
    return out


def controller(e, state):
    p = e.r32(PLAYER)
    if not p or e.r32(p) != 0x0804E1B4:
        return 0
    px, py, pz, h = s32v(e.r32(p + 0x1A0)) >> 8, s32v(e.r32(p + 0x1A4)) >> 8, s32v(e.r32(p + 0x1A8)), s32v(e.r32(p + 0x1AC))
    es = [r for r in enc(e) if r[2] > 0]
    if not es:
        return 0
    if state.get('t') is None:
        state['t'] = min(range(len(es)), key=lambda i: ((es[i][0] >> 8) - px) ** 2 + ((es[i][1] >> 8) - py) ** 2)
        es_all = enc(e)
        state['t'] = es_all.index(es[state['t']])
    t = enc(e)[state['t']]
    dx, dy = (t[0] >> 8) - px, (t[1] >> 8) - py
    want = math.atan2(dx, -dy) / (2 * math.pi) * 256
    diff = (want - (h / 1024) + 128) % 256 - 128
    k = 0
    if diff > 3:
        k |= KEY.RIGHT
    elif diff < -3:
        k |= KEY.LEFT
    dist = math.hypot(dx, dy)
    if abs(diff) < 30 and dist > 6:
        k |= KEY.UP
    if pz > t[2] + 4096:
        k |= KEY.A
    elif pz < t[2] - 4096:
        k |= KEY.B
    return k


def pass1():
    orig = cf.run_frame
    log = []
    st = {'go': False}

    def rf(e):
        a = area(e)
        if a[:2] == [0, 50] and e.r32(PLAYER) and e.r32(e.r32(PLAYER)) == 0x0804E1B4:
            st['go'] = True
        if st['go']:
            k = controller(e, st)
            e.set_keys(k)
        info, hs = orig(e)
        if st['go']:
            log.append(e.r32(0x04000130) and 0 or 0)  # placeholder (keys logged below)
            log[-1] = st.get('k', 0)
        return info, hs

    # log the keys actually set: wrap set_keys
    def wrap(e):
        f = e.set_keys

        def sk(k):
            st['k'] = k
            f(k)
        e.set_keys = sk

    cf.run_frame = rf
    try:
        import common
        orig_emu = cf.emu

        def emu2(s):
            e = orig_emu(s)
            wrap(e)
            return e
        cf.emu = emu2
        t = cf.record('enc_pass1', 'buu_snakeway_start.ss', rw.setup_capsule_8_1, [[0, N]], '', max_setup=900, frame0_load=2)
    finally:
        cf.run_frame = orig
        cf.emu = orig_emu
    # the logged keys of the recorded frames = the last N entries
    keys = log[-N:]
    loads = t['loads']
    print('pass1 loads', [(L['f'], L['to']) for L in loads], 'target', st.get('t'))
    return keys, loads


def compress(keys):
    out = []
    for k in keys:
        if out and out[-1][0] == k:
            out[-1][1] += 1
        else:
            out.append([k, 1])
    return out


if __name__ == '__main__':
    keys, loads = pass1()
    assert loads, 'no encounter fired'
    end = loads[0]['f'] + 120
    keys = keys[:end]
    inputs = compress(keys)
    scn = {'name': 'encounter', 'setup': rw.setup_capsule_8_1, 'input': inputs,
           'notes': 'world map from the 8-1 capsule: a steering controller (wm_encounter.py pass 1) flies into the '
                    'nearest airborne type-141 encounter, matching its altitude, until IfPlayerOnTop fires; then the '
                    'battle event takes over (warp to 0-55).'}
    # pass 2 with the extra rows
    orig = cf.run_frame
    extra = []

    def rf(e):
        ids = [e.breakpoint(a) for a in BPS]
        info, hs = orig(e)
        for i in ids:
            e.clear(i)
        hits = [BPS[h['addr'] & ~1] for h in hs if h['kind'] == 'break' and (h['addr'] & ~1) in BPS]
        extra.append((enc(e), hits))
        return info, hs
    cf.run_frame = rf
    try:
        t = rw.run(scn)
    finally:
        cf.run_frame = orig
    tail = extra[-len(t['frames']):]
    for fr, (en, hits) in zip(t['frames'], tail):
        fr['enc'] = en
        fr['hits'] = hits
    t['format_extra']['enc'] = '[x 24.8, y 24.8, z 22.10, AI top state, controller vt] per enemy-list entry'
    t['format_extra']['hits'] = 'encounter-related function entries hit in the frame'
    json.dump(t, open(ROOT / 'data/field/worldmap_encounter.json', 'w'), separators=(',', ':'))
    print('loads', [(L['f'], L['to'], L['dest']) for L in t['loads']])
