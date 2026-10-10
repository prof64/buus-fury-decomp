"""Emulator animation traces of selected enemies / bosses -> data/battle/buu_enemy_anim_traces.json, replayed tick by
tick by site/tests/battle/buu_enemy_anim.test.ts against the simulator's animation model (eanim.ts).

Source: the ea_rec recordings (work/buu/eanim, re/buu/ea_batch.py). Per trace, for the force-spawned enemy:
  rows   run-length [raw, frame, ticks] of what the game showed each logic tick (lag frames removed)
  defs   sprite def per row run when the enemy swaps graphics (Mechanoids, shield soldiers)
  init   [raw, frame, tick, controller] at tick 0
  cmds   [tick, phase, kind, ...] the animation calls the game made for this actor:
           ['anim', id]                 SetAnim(id)          (0x0800ECF0, vfunc +0x28)
           ['state', controller, raw]   SetAnimRaw(raw - 1) and / or SetState(controller)
           ['hold', id, frame]          SetAnimHold          (0x0800ED3E, vfunc +0x30)
         phase 'pre' = before the actor's own state tick (another actor's update: hits), 'post' = after (its AI)
  rand   per tick, the values the game's rand() returned to the animation layer: idle start ticks, swing sounds
  sfx    [tick, id] sounds made by the animation layer (punch swing 0x0800DF1A, the controllers' own cues)
  nframes {def: {raw: frames}}, canon {def: {raw: lowest raw with the same animation struct}} (what the recorder
         reports as raw)

usage: .venv/bin/python tools/battle/gen_buu_enemy_anim_traces.py
"""
import contextlib
import io
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
with contextlib.redirect_stdout(io.StringIO()):
    from common import u8, u16, u32, WORK  # noqa: E402
    from vtab import entries  # noqa: E402
    import ea_seg  # noqa: E402

OUT = os.path.join(ROOT, 'data', 'battle', 'buu_enemy_anim_traces.json')
TYPES = {
    # regular archetypes
    0x44: 'Majin Fighter (melee, back-off, block)', 0x2C: 'Gunman (gun controller)', 0x03: 'Atomizer (ki blast controller)',
    0x29: 'Grand Mechanoid (form swap)', 0x4E: 'Ninja (shuriken, teleport)', 0x17: 'Elite Shield Soldier (shield graphics swap)',
    0x09: 'Bones (wind-up melee)', 0x13: 'Death Machine (flame)',
    # bosses
    0x1C: 'Majin Buu (West City)', 0x73: 'Super Buu (Time Chamber)', 0x3C: 'Kid Buu', 0x33: 'Janemba', 0x7A: 'Super Janemba',
    0x0A: 'Broly', 0x12: 'Dabura', 0x5B: 'Pikkon', 0x91: 'Yakon', 0x6D: 'Spopovich', 0x46: 'Majin Vegeta',
}
SCENARIOS = ['near', 'mid', 'punch', 'blast', 'move']
AUTO = {0x0800DEA4, 0x0800DEDA}   # the state's own end callback (SetAnim(0) / hold) - the model does these itself
PUNCH_SFX = 0x0800DF1A


def ckey(vt, st):
    return '0x%08X@%s' % (vt, 'heap' if 0x02000000 <= st < 0x03000000 else '0x%08X' % st)


def fn_start(a):
    a &= ~1
    for k in range(0, 0x800, 2):
        if (u16(a - k) & 0xFF00) == 0xB500:
            return a - k
    return None


def nframes(defp, raw):
    return ea_seg.nframes(defp, raw)


def canon(defp, raw):
    a = u32(defp + 0x50 + 4 * raw)
    for r in range(u8(defp) + 2):
        if u32(defp + 0x50 + 4 * r) == a:
            return r
    return raw


def main():
    traces = []
    for t, label in TYPES.items():
        for scn in SCENARIOS:
            p = WORK / 'eanim' / ('%02x_%s.json' % (t, scn))
            if not p.exists():
                continue
            d = json.load(open(p))
            lt = d['lt']
            lag = set(i for i in range(1, len(lt)) if lt[i] == lt[i - 1])
            tgt = None
            for k, o in d['objs'].items():
                if o['type'] == t and o['rows'] and (o['rows'][0][8] >> 8) != 0x7000 and o['first'] == 0:
                    tgt = (int(k, 16), o)
                    break
            if not tgt:
                continue
            oid, o = tgt
            rows = [r for r in o['rows'] if r[0] not in lag]
            if len(rows) < 30:
                continue
            fidx = {r[0]: i for i, r in enumerate(rows)}
            evs = {}
            for ev in d['events']:
                evs.setdefault(ev[0], []).append(ev)
            defs = []
            nf = {}
            cn = {}
            for r in rows:
                df = r[13]
                key = '0x%08X' % df
                if not defs or defs[-1][0] != key:
                    defs.append([key, 1])
                else:
                    defs[-1][1] += 1
                if key not in nf:
                    nf[key] = {str(x): nframes(df, x) for x in range(1, u8(df) + 2)}
                    cn[key] = {str(x): canon(df, x) for x in range(1, u8(df) + 2)}
            rle = []
            for r in rows:
                if rle and rle[-1][0] == r[1] and rle[-1][1] == r[2]:
                    rle[-1][2] += 1
                else:
                    rle.append([r[1], r[2], 1])
            st0 = rows[0][5]
            init = [rows[0][1], rows[0][2], rows[0][3], ckey(rows[0][6], st0)]
            cmds, rnd, sfx = [], [], []
            for i, r in enumerate(rows):
                if i == 0:
                    continue
                f = r[0]
                es = evs.get(f, [])
                mine = [e for e in es if e[2] == oid and e[1] in ('anim', 'raw', 'state', 'hold', 'stateid')]
                pending_raw = None
                for e in mine:
                    kind, val, lr = e[1], e[3], e[4]
                    if lr in (0x0800ED16, 0x0800FAC4):   # SetAnim's own SetStateId / SetState
                        continue
                    if lr in AUTO:
                        continue
                    # a state's own tick ending it with SetAnim(0) (the model's 'end: idle')
                    pvt = rows[i - 1][6]
                    if kind == 'anim' and val == 0 and pvt and fn_start(lr) == (entries(pvt, 2)[1] & ~1):
                        cmds.append([i, 'end', 'end', 0, lr])
                        continue
                    if kind == 'anim':
                        cmds.append([i, None, 'anim', val, lr])
                    elif kind == 'raw':
                        pending_raw = val + 1
                        cmds.append([i, None, 'raw', val + 1, lr])
                    elif kind == 'state':
                        vt = e[5] if len(e) > 5 else 0
                        cmds.append([i, None, 'state', ckey(vt, val), lr])
                    elif kind == 'hold':
                        cmds.append([i, None, 'hold', val, e[5], lr])
                # idle entries draw rand(255): the start tick the game got
                # (several idle entries in one tick: only the last start tick is visible -> null for the others)
                idles = [e for e in mine if e[1] == 'state' and e[3] == 0x03001B0C]
                for k in range(len(idles)):
                    rnd.append([i, 'idle', r[3] if k == len(idles) - 1 else None])
                for j, e in enumerate(es):
                    if e[1] == 'sfx' and e[4] == PUNCH_SFX:
                        # the swing belongs to the actor whose punch state was entered just before it
                        st = [x for x in es[:j] if x[1] == 'state' and x[3] == 0x03001B50]
                        if st and st[-1][2] == oid:
                            sfx.append([i, e[3]])
                            rnd.append([i, 'swing', e[3] - 50])
            # phase: a generic one-shot / loop entered by another actor has ticked once already (tick 1)
            for c in cmds:
                i = c[0]
                r = rows[i]
                if c[1] == 'end':
                    continue
                c[1] = 'post'
                if c[2] in ('anim', 'state') and r[3] == 1 and r[6] in (0x0804D784, 0x0804C35C, 0x0804A1D0, 0x08049E40, 0x0804E0AC, 0x0804A730):
                    c[1] = 'pre'
            # idle start ticks: post -> the row's tick; pre -> one less
            for x in rnd:
                if x[1] == 'idle' and x[2] is not None:
                    pre = any(c[0] == x[0] and c[1] == 'pre' for c in cmds)
                    if pre:
                        x[2] -= 1
            # ticks the actor did not get (or got twice): the world update loop (0x08011A1E, descending over a list
            # that objects leave during the loop) skips / repeats an object when one below it is removed
            ticking = (0x0804A77C, 0x0804A1D0, 0x0804D784, 0x0804C35C, 0x0804A730, 0x0804E0AC, 0x08049E40, 0x0804AE14, 0x0804CC80)
            skips = []
            busy = {c[0] for c in cmds}
            for i in range(1, len(rows)):
                a, b = rows[i - 1], rows[i]
                if i in busy or a[6] != b[6] or a[5] != b[5] or b[6] not in ticking:
                    continue
                dt = (b[3] - a[3]) & 0xFFFF
                if b[6] in (0x0804AE14, 0x0804CC80):
                    dt = (a[3] - b[3]) & 0xFFFF
                if dt == 0 and not (b[6] == 0x0804E0AC and False):
                    skips.append([i, 0])
                elif dt == 2:
                    skips.append([i, 2])
                elif b[6] == 0x0804A77C and dt > 2:
                    # the idle state re-entered without a SetState call: 0x0800EB60 (sets the sprite def, reloads its
                    # graphics) calls the current state's enter -> a new rand(255) start tick
                    skips.append([i, 'reenter', b[3]])
            traces.append({'name': '%02x_%s' % (t, scn), 'skips': skips, 'type': t, 'label': label, 'scenario': scn, 'init': init,
                           'rows': rle, 'defs': defs, 'cmds': [c[:-1] for c in cmds], 'rand': rnd, 'sfx': sfx,
                           'nframes': nf, 'canon': cn})
    with open(OUT, 'w') as f:
        json.dump({'source': 'tools/battle/gen_buu_enemy_anim_traces.py (recordings re/buu/ea_batch.py)', 'traces': traces}, f,
                  separators=(',', ':'))
    print('wrote', OUT, len(traces), 'traces')


if __name__ == '__main__':
    main()
