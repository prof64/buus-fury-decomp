"""Opponent visual audit - the emulator side (headless, muted). Records lockstep enemy scenarios with, per frame, the
screen and every enemy's animation / AI, for the comparer `cargo run --release -p wf-app --example opp_audit`.

usage: .venv/bin/python tools/battle/lockstep/visual.py NAME_GLOB.. [-j N] [--out DIR]
  NAME_GLOB  scenario names from opp_scenarios.py (fnmatch, e.g. 'oa_t44_*'); `--list` prints them
  --out      output directory (default work/ub/oa)

Writes per scenario, in DIR:
  NAME.json  a lockstep trace (record.py's encoding: the comparer sets the sim up from it exactly as
             wf-core/tests/lockstep.rs does) plus `vis`: per frame (1-based index = list index + 1)
             {"en": [[type, x, y, dir, raw, frame, tick, hp1024, ai, z, hittable, obj]..] every live object of the enemy
             list 0x03001BB0 in list order (the scenario's enemy, its parts / summons),
              "ob": [[vtable, x, y]..] every other object of the world list 0x03001C4C (shots, effects, flashes;
             x / y only for actors), "cam": [x, y] (22.10)}
  NAME.fb    the screen after every frame (frame 0 = before the first input): a JSON header line
             {"frames": [0..n], "sizes": [...]} then zlib'd 240x160 RGB per frame (work/superbuu/pix/fb format)

Scenarios: opp_scenarios.py (one list per opponent type: idle, approach from each side, punch, ki, KO - see there).
A 600-frame scenario records in ~20 s.
"""
import fnmatch
import json
import os
import sys
import zlib
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from emu_lockstep import record, ROOT, ENEMY_LIST, anim_raw, s32v  # noqa: E402
from record import encode  # noqa: E402

OBJW = 0x03001C4C
TYPE_TAB = 0x08711274
STATE_T = 0x0870F514
PLAYER_VT = 0x0804C9F0
ENEMY_VT = 0x0804C484
HUD_VT = 0x08049FEC
ACTOR_VTS = {PLAYER_VT, ENEMY_VT, 0x0804C92C, 0x0804CEF4}
CAM_PTR = 0x080707E8


def s16(v):
    return v - 0x10000 if v & 0x8000 else v


def ai_top(e, o):
    n = e.r32(o + 0x23C)
    if not 0 < n < 16:
        return -1
    d = e.r32(o + 0x238 + 8 * n + 4)
    if STATE_T <= d < STATE_T + 8 * 4000 and (d - STATE_T) % 8 == 0:
        return (d - STATE_T) // 8
    return -1


def enemies(e):
    out = []
    n = e.r32(ENEMY_LIST)
    for k in range(min(n, 64)):
        o = e.r32(ENEMY_LIST + 4 + 4 * k)
        if not (0x02000000 <= o < 0x04000000):
            continue
        x = s32v(e.r32(o + 0x1A0))
        if (x >> 8) >= 0x6C00:  # parked spawn records
            continue
        out.append([(e.r32(o + 0x224) - TYPE_TAB) // 28, x, s32v(e.r32(o + 0x1A4)), e.r8(o + 0x10), anim_raw(e, o), e.r8(o + 0x11),
                    e.r16(o + 0x12), s32v(e.r32(o + 0x28C)), ai_top(e, o), s16(e.r16(o + 0x176)), e.r8(o + 0x1CC), o])
    return out


def others(e):
    out = []
    n = e.r32(OBJW + 4)
    for k in range(min(n, 255)):
        o = e.r32(OBJW + 8 + 4 * k)
        vt = e.r32(o)
        if vt in (PLAYER_VT, ENEMY_VT, HUD_VT):
            continue
        if vt in ACTOR_VTS:
            out.append([vt, s32v(e.r32(o + 0x1A0)) >> 8, s32v(e.r32(o + 0x1A4)) >> 8])
        else:
            out.append([vt, None, None])
    return out


def one(scn, out_dir):
    vis = []
    blobs = []

    def hook(e, f, r, eo):
        c = e.r32(CAM_PTR)
        vis.append({'en': enemies(e), 'ob': others(e), 'cam': [e.r32(c), e.r32(c + 4)]})
        blobs.append(zlib.compress(e.pixels(), 6))

    t = record(scn, hook=hook)
    enc = encode(t)
    enc['vis'] = vis[1:]
    enc['vis0'] = vis[0]
    name = scn['name']
    with open(os.path.join(out_dir, name + '.json'), 'w') as f:
        json.dump(enc, f, separators=(',', ':'))
    with open(os.path.join(out_dir, name + '.fb'), 'wb') as f:
        f.write((json.dumps({'frames': list(range(len(blobs))), 'sizes': [len(b) for b in blobs]}) + '\n').encode())
        for b in blobs:
            f.write(b)
    return '%s: %d frames' % (name, len(t['frames']))


def main():
    a = sys.argv[1:]
    j = 4
    out_dir = os.path.join(ROOT, 'work', 'ub', 'oa')
    if '-j' in a:
        j = int(a[a.index('-j') + 1]); del a[a.index('-j'):a.index('-j') + 2]
    if '--out' in a:
        out_dir = a[a.index('--out') + 1]; del a[a.index('--out'):a.index('--out') + 2]
    from opp_scenarios import SCENARIOS
    if '--list' in a:
        for s in SCENARIOS:
            print(s['name'])
        return
    todo = [s for s in SCENARIOS if any(fnmatch.fnmatch(s['name'], p) for p in a)]
    os.makedirs(out_dir, exist_ok=True)
    if j <= 1 or len(todo) <= 1:
        for s in todo:
            print(one(s, out_dir), flush=True)
        return
    with ProcessPoolExecutor(j) as ex:
        for line in ex.map(one, todo, [out_dir] * len(todo)):
            print(line, flush=True)


if __name__ == '__main__':
    main()
