"""How Buu's Fury enemies and bosses animate and what they play -> data/battle/buu_enemy_anim.json.

Inputs
  * ROM: the enemy SetAnim tables (raw 0x08072670, state 0x080726B8), state-object vtables, the AI node registry
    (data/buu/enemy_ai.json create functions -> runtime vtables) to name call sites.
  * Emulator recordings work/buu/eanim/<type>_<scenario>.json made by re/buu/ea_batch.py (re/buu/ea_rec.py): every enemy
    type force-spawned, 6 scenarios x 900 frames (far / near / punch / kill / move / blast), per frame each enemy's
    raw anim, frame, tick, facing, state object, AI state; every SetAnim / SetAnimRaw / SetState / PlaySfx call with
    its caller; the logic tick counter (lag frames are dropped: the game skipped its update on those video frames).

Model (tools/battle/ENEMY_ANIM.md): the shown frame is obj+0x11 of the animation struct obj+0x50 (raw = sprites.json
raw_index); the actor's state object obj+0x14 advances it once per logic tick before the AI runs. Generic state
objects follow the shared tick rules (`ticks`); attack / special controllers are per-node state objects whose frame
sequences are given as observed run-lengths (all complete observations must agree to be `deterministic`).

usage: .venv/bin/python tools/battle/gen_buu_enemy_anim.py
"""
import contextlib
import glob
import io
import json
import os
import sys
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
with contextlib.redirect_stdout(io.StringIO()):
    from common import emu, u8, u16, u32, s32, WORK  # noqa: E402
    from vtab import entries  # noqa: E402
    import ea_seg  # noqa: E402

REC_DIR = WORK / 'eanim'
OUT = os.path.join(ROOT, 'data', 'battle', 'buu_enemy_anim.json')
ANIM_RAW = 0x08072670
ANIM_STATE = 0x080726B8
SFX_TABLE = 0x083192A8

TICK_FN = {0x0800DE6C: 'once', 0x0800DEB0: 'loop', 0x0800DFD4: 'idle', 0x0800DF8A: 'bored', 0x0804E4E0: 'hold',
           0x08018C24: 'reverse', 0x0802F224: 'reverse'}
ENTER_FN = {0x0800DE60: 'tick = frame = 0', 0x0800DEDC: 'tick = frame = 0', 0x0800DFBE: 'tick = rand(255), frame 0',
            0x0800DEE8: 'tick = frame = 0; PlaySfx(50 + rand(2), 64)'}
END_FN = {0x0800DE94: 'SetAnim(0)', 0x0800DED0: 'hold state 0x03001B00 (last frame kept)', 0x0800DEA6: 'none (frame kept, tick runs on)'}
TICKS = {
    'once': '0x0800DE6C: tick += 1; f = tick >> shift; f >= nframes -> end callback (vtable +0x24), frame unchanged; else frame = f',
    'loop': '0x0800DEB0: tick += 1; f = tick >> shift; f >= nframes -> tick = 0, f = 0; frame = f',
    'idle': '0x0800DFD4: tick += 1; nframes == 2: frame = (tick & 0xFF) > 0xF0 ? 1 : 0 (blink 15 of 256); else frame = (tick >> 3) & 0xFF, past the end -> frame = tick = 0',
    'bored': '0x0800DF8A: tick += 1; f = tick >> 5; f >= nframes -> tick = 0x40, frame = 2; else frame = f',
    'reverse': '0x08018C24 / 0x0802F224: tick -= 1; tick < 0 -> tick = nframes * 8 - 1; frame = tick >> 3 (the walk cycle played backwards, 8 ticks per frame)',
    'hold': '0x0804E4E0: nothing (frame kept)',
}
# call sites -> what they are (decoded from the code around them; the AI node names come from enemy_ai.json)
SITE_NAMES = {
    0x0800DEA4: 'one-shot end -> SetAnim(0) (0x0800DE94)',
    0x0800E05C: 'loop state slot 4 -> SetAnim(0)',
    0x0800DF1A: 'punch state enter (0x0800DEE8): swing sound',
    0x0802A258: 'MeleeAttack hit test 0x0802A0C4: hit landed on the player',
    0x0802A272: 'MeleeAttack: SetAnim(4) after the hit test',
    0x08012AA4: 'player melee 0x08012A24: a hit landed (player sound)',
    0x08018A8E: 'shared hit reaction (aux tree of state 0, update 0x08018A92): hurt SetAnim(9)',
    0x080187E6: 'Block node: SetAnim(7) guard',
    0x0801984A: 'ki blast ctor 0x080197B4: launch sound',
    0x080199E0: 'ki blast hit 0x08019888',
}
SFX_PLAYER_SITES = {0x08012AA4}   # always the player's own sound


def fn_start(a):
    a &= ~1
    for k in range(0, 0x800, 2):
        if (u16(a - k) & 0xFF00) == 0xB500:
            return a - k
    return None


def create_vtable(fn):
    """last ROM vtable literal (0x08049000..0x08050000) loaded by a node create function"""
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
    fn &= ~1
    from common import rom, ra
    vts = []
    for ins in md.disasm(rom()[ra(fn):ra(fn) + 200], fn):
        if ins.mnemonic == 'ldr' and 'pc' in ins.op_str:
            off = int(ins.op_str.split('#')[1].rstrip(']'), 16)
            lit = u32(((ins.address + 4) & ~3) + off)
            if 0x08049000 <= lit < 0x08050000:
                vts.append(lit)
        if ins.mnemonic == 'pop' and 'pc' in ins.op_str:
            break
    return vts[-1] if vts else None


def node_functions():
    ai = json.load(open(os.path.join(ROOT, 'data', 'buu', 'enemy_ai.json')))
    fn = defaultdict(list)
    for nt in ai['node_types']:
        cf = int(nt['create_fn'], 16)
        fn[cf & ~1].append('%s.create' % nt['name'])
        vt = create_vtable(cf)
        if vt:
            for s, f in enumerate(entries(vt, 12)):
                if 0x08000000 <= f < 0x08800000:
                    fn[f & ~1].append('%s[%d]' % (nt['name'], s))
    return fn, ai


def site_name(lr, FN):
    if lr in SITE_NAMES:
        return SITE_NAMES[lr]
    s = fn_start(lr)
    n = FN.get(s, [])
    if s in TICK_FN:
        n = n + ['tick ' + TICK_FN[s]]
    return ', '.join(n) if n else ('fn 0x%08X' % s if s else '?')


def h(v):
    return '0x%08X' % v


def ckey(vt, st):
    """controller key 'vtable@state object'; per-instance (heap) state objects are keyed '@heap'"""
    return '%s@%s' % (h(vt), 'heap' if 0x02000000 <= (st or 0) < 0x03000000 else h(st))


def setanim_table(e):
    out = []
    for i in range(23):
        st = u32(ANIM_STATE + 4 * i)
        vt = e.r32(st)
        ents = entries(vt, 11)
        tick = TICK_FN.get(ents[1] & ~1, 'custom %s' % h(ents[1] & ~1))
        shift = e.r8(st + 4) if tick in ('once', 'loop') else (3 if tick == 'idle' else None)
        out.append({'id': i, 'raw': u8(ANIM_RAW + i) + 1, 'state': h(st), 'vtable': h(vt), 'tick': tick, 'shift': shift,
                    'enter': ENTER_FN.get(ents[0] & ~1, h(ents[0] & ~1)),
                    'end': END_FN.get(ents[9] & ~1, h(ents[9] & ~1)) if tick == 'once' else None,
                    'frames_per_step': (1 << shift) if shift is not None and tick in ('once', 'loop') else None})
    return out


def rle(seq):
    out = []
    for x in seq:
        if out and out[-1][0] == x[0] and out[-1][1] == x[1]:
            out[-1][2] += 1
        else:
            out.append([x[0], x[1], 1])
    return out


def main():
    FN, ai = node_functions()
    e = emu('buu_fight_swforest.ss')
    e.run(2)
    setanim = setanim_table(e)
    sa_by_state = {}
    for s in setanim:
        sa_by_state.setdefault(int(s['state'], 16), s)
    sprites = json.load(open(os.path.join(ROOT, 'data', 'buu', 'sprites.json')))
    def_sprite = {}
    for ent in sprites['entities']:
        d = (ent.get('rom') or {}).get('def')
        if d:
            def_sprite[int(d, 16)] = ent
    enemies = json.load(open(os.path.join(ROOT, 'data', 'buu', 'enemies.json')))['enemies']
    audio = json.load(open(os.path.join(ROOT, 'data', 'buu', 'audio.json')))
    sfx_name = {s['id']: s['name'] for s in audio['sfx']}

    # ---- collect segments + sfx from every recording
    per_type = defaultdict(lambda: {'segs': defaultdict(list), 'sfx': defaultdict(lambda: {'n': 0, 'ids': Counter(), 'vol': Counter(), 'at': Counter(), 'scn': Counter()}),
                                    'defs': Counter(), 'scn': Counter(), 'deaths': []})
    controllers = defaultdict(lambda: {'types': Counter(), 'raws': Counter(), 'sites': Counter()})
    files = sorted(glob.glob(str(REC_DIR / '*_*.json')))
    stats = Counter()
    for path in files:
        d = json.load(open(path))
        if 'events' not in d or 'lt' not in d:
            continue
        scn = d['scenario']
        lt = d['lt']
        pobj = d.get('pobj')
        obj_type = {int(k, 16): o['type'] for k, o in d['objs'].items()}
        segs = ea_seg.segments(d)
        # the segment of each object at each frame, for sfx offsets
        seg_at = {}
        seg_frame = defaultdict(list)
        for s in segs:
            if s['obj'] == pobj or s['type'] is None:
                continue
            t = s['type']
            T = per_type[t]
            T['defs'][s['deff']] += 1
            first = s['rows'][0]
            raw0 = first[1]
            entry = s['entry']
            stn = 0x02000000 if 0x02000000 <= (s['st'] or 0) < 0x03000000 else s['st']   # heap objects: one key
            key = (s['stvt'], stn, raw0)
            # who set it: the first anim / raw / hold / stateid / state call whose caller is not the engine's wrappers
            # (the call chain that ended in the frame's last SetState for this actor; earlier calls in the same
            # frame were overridden)
            via = None
            last_state = max([i for i, ev in enumerate(entry) if ev[0] == 'state'], default=-1)
            calls = [ev for ev in entry[:last_state + 1] if ev[0] != 'spawn' and ev[2] not in (0x0800ED16, 0x0800FAC4)]
            if calls:
                ev = calls[-1]
                via = (ev[0], ev[1] if ev[0] != 'state' else None, ev[2])
                if ev[0] == 'state' and len(calls) > 1 and calls[-2][0] in ('raw', 'hold') and fn_start(calls[-2][2]) == fn_start(ev[2]):
                    via = ('raw+state', calls[-2][1], ev[2])
            info = {'rows': [(r[1], r[2]) for r in s['rows']], 'ticks': [r[3] for r in s['rows']], 'via': via,
                    'complete': s['complete'], 'entered': s['entered'], 'ai': s['ai'], 'scn': scn, 'start': s['start'],
                    'lt0': lt[s['start']] if s['start'] < len(lt) else None, 'check': ea_seg.check(s),
                    'next': s['next'], 'dirs': [r[4] for r in s['rows']], 'def': s['deff']}
            T['segs'][key].append(info)
            c = controllers[(s['stvt'], stn)]
            c['types'][t] += 1
            c['raws'][raw0] += 1
            if via:
                c['sites'][via[2]] += 1
            for r in s['rows']:
                seg_at[(s['obj'], r[0])] = (key, s['start'])
                seg_frame[r[0]].append((s['obj'], (key, s['start'])))
        for k, o in d['objs'].items():
            T = per_type[o['type']]
            T['scn'][scn] += 1
        # sfx
        for ev in d['events']:
            if ev[1] != 'sfx':
                continue
            f, _, _, sid, lr, vol, pan = ev[:7]
            who = ev[7] if len(ev) > 7 else 0
            if lr in SFX_PLAYER_SITES or who == pobj:
                stats['sfx_player'] += 1
                continue
            # punch-enter sounds belong to the actor whose state was set just before in the same frame
            if lr == 0x0800DF1A and not who:
                prev = [x for x in d['events'] if x[0] == f and x[1] == 'state' and x[3] == 0x03001B50]
                who = prev[-1][2] if prev else 0
                if who == pobj:
                    stats['sfx_player'] += 1
                    continue
            if lr == 0x0800083E:
                stats['sfx_script'] += 1   # script native PlaySfx (level up after a kill)
                continue
            if not who:
                # the actor whose current state object's enter / tick function made the call
                fs = fn_start(lr)
                act = seg_frame.get(f, [])
                for o, sa in act:
                    vt = sa[0][0]
                    if vt and fs in [x & ~1 for x in entries(vt, 2)]:
                        who = o
                        break
                if not who and len({o for o, _ in act}) == 1:
                    who = act[0][0]
            t = obj_type.get(who)
            if t is None:
                # unattributed: in single-enemy scenarios it is the spawned type's (or its minions')
                t = d['type']
                stats['sfx_unattributed'] += 1
            T = per_type[t]
            key = (lr,)
            S = T['sfx'][key]
            S['n'] += 1
            S['ids'][sid] += 1
            S['vol'][vol] += 1
            S['scn'][scn] += 1
            sa = seg_at.get((who, f)) if who else None
            if sa:
                S['at'][(sa[0], lt[f] - lt[sa[1]])] += 1
                S.setdefault('at_ids', defaultdict(Counter))[(sa[0], lt[f] - lt[sa[1]])][sid] += 1
        stats['files'] += 1

    # ---- controllers catalog
    ctl_out = {}
    for (vt, st), c in sorted(controllers.items(), key=lambda x: (x[0][0] or 0, x[0][1] or 0)):
        if not vt:
            continue
        ents = entries(vt, 11)
        g = ea_seg.GENERIC.get(vt)
        key = ckey(vt, st)
        if key in ctl_out:
            prev = ctl_out[key]
            for r, n in c['raws'].items():
                prev['raws'][str(r)] = prev['raws'].get(str(r), 0) + n
            prev['types'] = sorted(set(prev['types']) | {'0x%02X' % t for t in c['types']})
            continue
        sa = sa_by_state.get(st)
        ctl_out[key] = {
            'vtable': h(vt), 'state_object': 'heap (per instance)' if key.endswith('@heap') else h(st),
            'enter': ENTER_FN.get(ents[0] & ~1, h(ents[0] & ~1)), 'tick': TICK_FN.get(ents[1] & ~1, h(ents[1] & ~1)),
            'end': END_FN.get(ents[9] & ~1, h(ents[9] & ~1)),
            'generic': g[0] if g else None,
            'shift': (ea_seg.ONCE_SHIFT.get(st, g[1]) if g and g[0] in ('once', 'once_stop') else
                      ea_seg.LOOP_SHIFT.get(st, g[1]) if g and g[0] == 'loop' else g[1] if g else None),
            'setanim_id': [s['id'] for s in setanim if int(s['state'], 16) == st] or None,
            'raws': {str(k): v for k, v in c['raws'].most_common()},
            'types': ['0x%02X' % t for t, _ in c['types'].most_common()],
            'set_by': [{'site': h(lr), 'name': site_name(lr, FN), 'n': n} for lr, n in c['sites'].most_common(12)],
        }

    # ---- per type
    ents_out = {}
    verify = Counter()
    sfx_sites = defaultdict(lambda: {'ids': Counter(), 'types': Counter(), 'n': 0})
    for t in range(150):
        E = enemies[t]
        T = per_type.get(t)
        defp = None
        if T and T['defs']:
            defp = T['defs'].most_common(1)[0][0]
        if defp is None and E.get('gfx_defs'):
            defp = int(list(E['gfx_defs'].values())[0], 16)
        spr = def_sprite.get(defp) if defp else None
        anim_names = {}
        if spr:
            for nm, a in spr['animations'].items():
                anim_names[a['raw_index']] = nm
        states = []
        if T:
            for (vt, st, raw0), L in sorted(T['segs'].items(), key=lambda x: -len(x[1])):
                g = ea_seg.GENERIC.get(vt)
                ok = sum(1 for x in L if x['check'][0] is not None and x['check'][2] is None)
                chk = sum(1 for x in L if x['check'][0] is not None)
                verify['generic_segments'] += chk
                verify['generic_ok'] += ok
                vias = Counter()
                for x in L:
                    if x['via']:
                        k, val, lr = x['via']
                        how = {'anim': 'SetAnim(%s)', 'raw': 'SetAnimRaw(%s)', 'hold': 'SetAnimHold(%s)', 'raw+state': 'SetAnimRaw(%s) + SetState',
                               'stateid': 'SetStateId(%s)', 'state': 'SetState'}[k]
                        vias[(how % val if '%s' in how else how, lr)] += 1
                    elif not x['entered']:
                        vias[('(already in this state when recording started)', 0)] += 1
                ais = Counter(x['ai'] for x in L if x['ai'] is not None)
                ent = {'controller': ckey(vt, st) if vt else None, 'raw': raw0,
                       'anim': anim_names.get(raw0), 'nframes': ea_seg.nframes(defp, raw0) if defp else None,
                       'observed': len(L),
                       'set_by': [{'call': c, 'site': h(lr) if lr else None, 'name': site_name(lr, FN) if lr else None, 'n': n}
                                  for (c, lr), n in vias.most_common(8)],
                       'ai_states': ['0x%X' % a for a, _ in ais.most_common(10)]}
                if g:
                    ent['rule'] = g[0]
                    ent['verified'] = '%d/%d segments follow the rule tick for tick' % (ok, chk)
                    if g[0] in ('once', 'once_stop'):
                        sh = ea_seg.ONCE_SHIFT.get(st, g[1])
                        n = ent['nframes'] or 0
                        ent['shift'] = sh
                        # AI-set: shown frames 0..n-1 for (1 << sh) ticks (frame 0 one tick less after the entry tick)
                        ent['ticks_total'] = (n << sh) if n else 1
                    elif g[0] == 'loop':
                        ent['shift'] = ea_seg.LOOP_SHIFT.get(st, g[1])
                    ent['entry_ticks'] = dict(Counter(str(x['ticks'][0]) for x in L if x['entered']).most_common(4)) if g[0] != 'idle' else 'rand(255)'
                else:
                    # custom controller: observed frame sequences per logic tick
                    comp = [x for x in L if x['complete'] and x['entered']]
                    seqs = Counter(json.dumps(rle(x['rows'])) for x in comp)
                    ent['rule'] = 'sequence'
                    if seqs:
                        best, nb = seqs.most_common(1)[0]
                        ent['sequence'] = json.loads(best)
                        ent['length'] = sum(r[2] for r in ent['sequence'])
                        ent['deterministic'] = len(seqs) == 1
                        ent['variants'] = len(seqs)
                        # interrupted runs (a hit, the AI leaving early) are prefixes of the full one
                        full = max((x['rows'] for x in comp), key=len)
                        pre = sum(1 for x in comp if x['rows'] == full[:len(x['rows'])])
                        ent['prefix_consistent'] = '%d/%d complete runs are prefixes of the longest' % (pre, len(comp))
                        if pre == len(comp) and len(seqs) > 1:
                            ent['sequence'] = rle(full)
                            ent['length'] = len(full)
                        verify['custom_prefix_ok'] += pre
                        lens = sorted(set(len(x['rows']) for x in comp))
                        ent['lengths_seen'] = lens[:12]
                        verify['custom_segments'] += len(comp)
                        verify['custom_agree'] += nb
                    else:
                        ent['sequence'] = rle(max(L, key=lambda x: len(x['rows']))['rows'])
                        ent['deterministic'] = None
                    nxt = Counter()
                    for x in comp:
                        for k, val, lr in (x['next'] or []):
                            if k in ('anim', 'raw', 'state', 'stateid', 'hold'):
                                nxt[('%s(%s)' % (k, hex(val) if k == 'state' else val), lr)] += 1
                                break
                    ent['exit'] = [{'call': c, 'site': h(lr), 'name': site_name(lr, FN), 'n': n} for (c, lr), n in nxt.most_common(4)]
                    # runs the state ended itself (its own tick called SetAnim): one length = fixed
                    tickfn = entries(vt, 2)[1] & ~1 if vt else None
                    own = sorted({len(x['rows']) for x in comp
                                  if any(k in ('anim', 'raw', 'state', 'stateid', 'hold') and fn_start(lr) == tickfn
                                         for k, val, lr in (x['next'] or [])[:1])})
                    ent['self_end_lengths'] = own[:12]
                    if len(own) == 1:
                        runs = Counter(json.dumps(rle(x['rows'])) for x in comp if len(x['rows']) == own[0])
                        if runs:
                            ent['sequence'] = json.loads(runs.most_common(1)[0][0])
                            ent['length'] = own[0]
                # sfx cues at tick offsets within this state
                cues = []
                for (lr,), S in T['sfx'].items():
                    for (key2, off), n in S['at'].items():
                        if key2 == (vt, st, raw0):
                            ids = S['at_ids'][(key2, off)]
                            cues.append({'at': off, 'site': h(lr), 'name': site_name(lr, FN), 'n': n,
                                         'ids': {str(k): v for k, v in sorted(ids.items())}, 'lr': lr})
                if cues:
                    cues.sort(key=lambda c: (c['at'], -c['n']))
                    ent['sfx_at'] = [{k: v for k, v in c.items() if k != 'lr'} for c in cues[:12]]
                    # cues the state object itself makes (its enter / tick function) in most complete runs
                    if vt and g is None:
                        own = [x & ~1 for x in entries(vt, 2)]
                        ncomp = max(1, sum(1 for x in L if x['complete'] and x['entered']))
                        ent['own_sfx'] = []
                        for c in cues:
                            if fn_start(c['lr']) in own and c['n'] >= 0.8 * ncomp:
                                ids = sorted(int(k) for k in c['ids'])
                                cue = {'at': c['at'], 'id': ids[0], 'vol': 64}
                                if len(ids) == 2 and ids[1] == ids[0] + 1:
                                    cue['rand'] = 2
                                ent['own_sfx'].append(cue)
                states.append(ent)
        sfx = []
        if T:
            for (lr,), S in sorted(T['sfx'].items(), key=lambda x: -x[1]['n']):
                ids = S['ids']
                sfx.append({'site': h(lr), 'name': site_name(lr, FN), 'n': S['n'],
                            'ids': {str(k): v for k, v in ids.most_common()},
                            'id_names': {str(k): sfx_name.get(k) for k in ids},
                            'vol': list(S['vol'])[0] if len(S['vol']) == 1 else dict(S['vol']),
                            'scenarios': dict(S['scn'])})
                SS = sfx_sites[lr]
                SS['n'] += S['n']
                SS['ids'].update(ids)
                SS['types'][t] += S['n']
        # compact rules for the simulator: per raw, the state the game most often shows it in (entered by a call);
        # per state object ('vtable@object'), the rule of its most observed use
        sim = {}
        best = {}
        bestc = {}

        def rule_of(s):
            vt = int(s['controller'].split('@')[0], 16)
            ends = END_FN.get(entries(vt, 11)[9] & ~1, '')
            end = 'idle' if ends.startswith('SetAnim(0)') else 'hold' if ends.startswith('hold') else 'none' if ends.startswith('none') else None
            r = {'rule': s['rule']}
            if s['rule'] in ('once', 'once_stop', 'loop'):
                r['shift'] = s.get('shift', 3)
            if s['rule'] in ('once', 'once_stop'):
                r['end'] = end or 'none'
            if s['rule'] == 'sequence':
                own = s.get('self_end_lengths') or []
                r = {'rule': 'seq', 'seq': s['sequence'], 'fixed': len(own) == 1 and own[0] == s.get('length')}
                if s.get('own_sfx'):
                    r['sfx'] = s['own_sfx']
                ex = s.get('exit') or []
                # ends itself (its own tick calls SetAnim(0)); otherwise the AI node ends it
                if ex and ex[0]['call'] == 'anim(0)' and fn_start(int(ex[0]['site'], 16)) == (entries(vt, 2)[1] & ~1):
                    r['end'] = 'idle'
            if vt == 0x0804C35C:
                r['swing'] = True
            r['controller'] = s['controller']
            return r

        for s in states:
            if s['raw'] is None or s['raw'] < 0 or not s['controller']:
                continue
            n_ent = sum(x['n'] for x in s['set_by'] if x['site'])
            if n_ent == 0:
                continue
            if bestc.get(s['controller'], -1) < n_ent:
                bestc[s['controller']] = n_ent
                sim[s['controller']] = rule_of(s)
            if s['raw'] in best and best[s['raw']] >= n_ent:
                continue
            best[s['raw']] = n_ent
            sim[str(s['raw'])] = rule_of(s)
        ents_out['0x%02X' % t] = {
            'sim': sim,
            'type': t, 'name': E.get('display_name') or E.get('name'),
            'sprite': spr['id'] if spr else None, 'def': h(defp) if defp else None,
            'anims': {str(r): {'name': anim_names.get(r), 'nframes': ea_seg.nframes(defp, r)} for r in sorted(anim_names)} if spr else {},
            'recorded': dict(T['scn']) if T else {},
            'states': states, 'sfx': sfx,
        }
    sfx_out = [{'site': h(lr), 'name': site_name(lr, FN), 'n': S['n'],
                'ids': {str(k): v for k, v in S['ids'].most_common()},
                'types': ['0x%02X' % t for t, _ in S['types'].most_common()]}
               for lr, S in sorted(sfx_sites.items(), key=lambda x: -x[1]['n'])]
    doc = {
        'source': 'tools/battle/gen_buu_enemy_anim.py; recordings re/buu/ea_batch.py (work/buu/eanim); doc tools/battle/ENEMY_ANIM.md',
        'conventions': {
            'raw': 'animation index in the sprite def (def+0x50 + 4*raw) = sprites.json raw_index; SetAnim(id) shows raw table[id] + 1, SetAnimRaw(n) shows raw n + 1',
            'tick': 'logic ticks (one game update; video lag frames removed). A state entered by the AI shows its first frame with tick 0 on the entering tick; one entered from another actor (a hit) has already ticked once (tick 1)',
            'order': 'per tick an enemy runs: the +0x18 controller, the state tick (vtable +4; may end -> SetAnim(0)), then the AI tree',
            'sequence': 'custom controllers: [raw, frame, ticks] run-lengths from the entering tick to the last tick before the next SetState',
            'dir': '0 down, 1 up, 2 left, 3 right',
        },
        'setanim': setanim,
        'ticks': TICKS,
        'controllers': ctl_out,
        'sfx_sites': sfx_out,
        'sfx_names': {str(k): v for k, v in sorted(sfx_name.items())},
        'verification': dict(verify, **{k: v for k, v in stats.items()}),
        'entities': ents_out,
    }
    with open(OUT, 'w') as f:
        json.dump(doc, f, indent=1)
    # the simulator's compact copy: per enemy type, raw -> state rule (site/src/battle/rulesets/buu/eanim.ts)
    sim = {'source': 'tools/battle/gen_buu_enemy_anim.py (the `sim` blocks of buu_enemy_anim.json)',
           'types': {k: v['sim'] for k, v in ents_out.items() if v['sim']}}
    with open(OUT.replace('.json', '_sim.json'), 'w') as f:
        json.dump(sim, f, separators=(',', ':'))
    print('wrote', OUT, dict(verify), dict(stats))


if __name__ == '__main__':
    main()
