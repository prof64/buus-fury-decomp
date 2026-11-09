"""report.py [--baseline]: the coverage report - what the game's content is, and how much of it ever appears in a frame
compared with the game. Reads the recorder's lines (work/coverage/raw/*.jsonl, written by tools/coverage/run.sh) and
the extracted content (engine/target/tmp/wf-import-segment-content, engine/annotations/script_natives.json); writes
engine/docs/coverage.md and data/coverage.json. `--baseline` also saves the shares as tools/coverage/baseline.json
(the "before" column of later reports). No ROM-derived data in the outputs: names, ids and counts only.

Levels, strongest first (an item's level is the strongest it was seen at):
  picture  in a picture that a gate compares with the game's: the pixel lock's frames (`locked`), the menu scenarios
           (`menu`), the link recordings (`link`)
  state    in a replay that a gate compares with the game's state every frame / iteration, no picture: the segment
           gate's matched frames (`state`), the three whole runs (`run`), the battle lockstep traces (`lockstep`)
  audit    in the by-hand audits in work/ (opponent and skill audits: every frame against the game, not gated here)
  board    on the picture board only (a sampled frame compared, not exact / outside the lock)
"How often a player meets it" = the logic iterations it was present in over the three whole runs (`run` counts)."""
import collections
import glob
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / 'work' / 'coverage' / 'raw'
C = ROOT / 'engine' / 'target' / 'tmp' / 'wf-import-segment-content'
GROUP = {'locked': 'picture', 'menu': 'picture', 'link': 'picture', 'state': 'state', 'run': 'state',
         'lockstep': 'state', 'audit': 'audit', 'board': 'board'}
ORDER = ['picture', 'state', 'audit', 'board', 'never']

# --baseline: the state before this pass's recordings (the `cov_*` segments are left out)
BASE = '--baseline' in sys.argv
seen = collections.defaultdict(dict)   # (cat, key) -> level -> [ctx, frame] of a first sight
ctxs = collections.defaultdict(set)    # (cat, key, level) -> contexts
met = collections.Counter()            # (cat, key) -> iterations in the whole runs
levels_present = set()
for f in sorted(glob.glob(str(RAW / '*.jsonl'))):
    for ln in open(f):
        try:
            r = json.loads(ln)
        except ValueError:
            continue
        if BASE and r.get('ctx', '').startswith('cov_'):
            continue
        k = (r['c'], r['k'])
        if 'n' in r:
            if r['l'] == 'run':
                met[k] += r['n']
            continue
        levels_present.add(r['l'])
        ctxs[k + (r['l'],)].add(r['ctx'])
        cur = seen[k].get(r['l'])
        if cur is None or (r['ctx'], r['f']) < tuple(cur):
            seen[k][r['l']] = [r['ctx'], r['f']]


def level_of(k):
    g = {GROUP[l] for l in seen.get(k, {})}
    return next(o for o in ORDER if o in g or o == 'never')


def where(k, n=3):
    """a few places an item is seen, strongest level first"""
    out = []
    for l in ('locked', 'menu', 'link', 'state', 'run', 'lockstep', 'audit', 'board'):
        if l in seen.get(k, {}):
            c = sorted(ctxs[k + (l,)])
            if l == 'run':
                out.append(f"run {seen[k][l][0]} row {seen[k][l][1]:,}")
            else:
                out.append(f"{l} {', '.join(c[:n])}" + (f" (+{len(c) - n})" if len(c) > n else ''))
            break
    return '; '.join(out)


def jl(p):
    return json.loads(Path(p).read_text())


# --- the game's content ---
cats = collections.OrderedDict()   # name -> {key: label}
natives = {n['id']: n for n in jl(ROOT / 'engine' / 'annotations' / 'script_natives.json')}
# (0x00..0x07 are the VM's own variable reads, never dispatched)
cats['native'] = {f"0x{i:02x}": f"{n['name']} ({n.get('uses', 0)} uses in the scripts)" for i, n in natives.items() if i >= 8 and n.get('uses', 0) > 0}
static_uses = {('native', f"0x{i:02x}"): n.get('uses', 0) for i, n in natives.items()}
sheets = {}
for d in ('effects', 'sprites'):
    for p in sorted((C / d).iterdir()):
        if p.is_dir():
            sheets[f'{d}/{p.name}'] = ''
cats['effect sheet'] = {k: v for k, v in sheets.items() if k.startswith('effects/') and not k.startswith('effects/props_')}
cats['prop sheet'] = {k: v for k, v in sheets.items() if k.startswith('effects/props_')}
cats['sprite sheet'] = {k: v for k, v in sheets.items() if k.startswith('sprites/')}
enemies = {}
for p in sorted(glob.glob(str(C / 'enemies' / '*.json'))):
    e = jl(p)
    if 'type' in e:
        enemies[e['type']] = e
cats['enemy'] = {str(t): e.get('name') or e['id'] for t, e in enemies.items()}
# an enemy type's AI states: its first state and every state its trees (and their on-hit trees) name
trees = {}
for p in glob.glob(str(C / 'ai' / 'state_*.json')):
    s = jl(p)
    trees[s['state']] = s


def states_in(v, out):
    if isinstance(v, dict):
        st = v.get('params', {}).get('state') if isinstance(v.get('params'), dict) else None
        if isinstance(st, dict) and isinstance(st.get('value'), int):
            out.add(st['value'])
        for x in v.values():
            states_in(x, out)
    elif isinstance(v, list):
        for x in v:
            states_in(x, out)


def reach(s0):
    todo, got = [s0], set()
    while todo:
        s = todo.pop()
        if s in got or s not in trees:
            continue
        got.add(s)
        nxt = set()
        states_in(trees[s].get('tree'), nxt)
        states_in(trees[s].get('aux'), nxt)
        todo += list(nxt)
    return got


cats['enemy AI state'] = {}
for t, e in enemies.items():
    for s in sorted(reach(e.get('ai_state', -1))):
        cats['enemy AI state'][f'{t}:{s}'] = f"{e.get('name') or e['id']} state 0x{s:x} ({(trees[s].get('text') or ['?'])[0].split('@')[0].strip()})"
forms = {}
for p in sorted(glob.glob(str(C / 'forms' / '*.json'))):
    f = jl(p)
    if 'index' in f:
        forms[f['index']] = f
skills = {}
for p in sorted(glob.glob(str(C / 'skills' / '*.json'))):
    s = jl(p)
    if 'skill' in s:
        skills[s['skill']] = s['name']
fname = lambda i: f"{forms[i]['name']} (form {i}, {forms[i].get('character') or '-'})" if i in forms else f'form {i}'
cats['form'] = {str(i): fname(i) for i in forms}
# a form's skills: no table in the content - every (form, skill) pair any replay or audit used
cats['skill (per form)'] = {}
for (c, k) in list(seen):
    if c == 'skill':
        fo, sk = (int(x) for x in k.split(':'))
        cats['skill (per form)'][k] = f"{skills.get(sk, f'skill {sk}')} as {fname(fo)}"
cats['skill'] = {str(i): n for i, n in skills.items()}
cats['transformation'] = {}
for i, f in forms.items():
    for to in (f.get('transform_up'), f.get('transform_down')):
        if isinstance(to, int) and to in forms and to != i:
            cats['transformation'][f'{i}>{to}'] = f"{fname(i)} -> {fname(to)}"
areas = jl(C / 'areas.json')['areas']
cats['area'] = {a['key']: a.get('name') or '' for a in areas}
cats['screen'] = {k: v for k, v in [
    ('Status', 'pause menu: Status'), ('Inventory', 'pause menu: Inventory'), ('Equip', 'pause menu: Equipment'),
    ('Skills', 'pause menu: Skills'), ('Journal', 'pause menu: Journal'), ('Options', 'pause menu: Options'),
    ('ShopBuy', 'shop: buy'), ('ShopSell', 'shop: sell'), ('Save', 'save / load'), ('CharSelect', 'Switch Characters'),
    ('GameOver', 'Game Over'), ('Credits', 'credits'), ('MinigamePower', 'mini-game: power gauge'),
    ('MinigamePunch', 'mini-game: punch'), ('MinigameWatch', 'mini-game: Watch This!'), ('Link', 'link pages (Dojo / Trade)'),
    ('WorldMapFlight', 'world map (flying)')]}
# open sets: what the game can show is not listed in the content, so the set is what any replay showed
OPEN = {'screen': 'screen', 'object': 'object (shots, effects objects)', 'part': 'screen part', 'visual': 'script visual',
        'player_state': 'player state', 'player_hold': 'held-by-enemy state'}
for (c, k) in list(seen):
    if c in OPEN and c != 'screen':
        cats.setdefault(OPEN[c], {})[k] = ''
    if c == 'screen':
        cats['screen'].setdefault(k, '')
RAWCAT = {'native': 'native', 'effect sheet': 'sheet', 'prop sheet': 'sheet', 'sprite sheet': 'sheet', 'enemy': 'enemy',
          'enemy AI state': 'enemy_state', 'form': 'form', 'skill (per form)': 'skill', 'skill': 'skill_any_form',
          'transformation': 'transform', 'area': 'area', 'screen': 'screen', **{v: k for k, v in OPEN.items()}}

out = {'levels': sorted(levels_present), 'categories': {}}
for name, items in cats.items():
    rc = RAWCAT[name]
    rows = []
    for key, label in items.items():
        k = (rc, key)
        rows.append({'key': key, 'label': label, 'level': level_of(k), 'met': met.get(k, 0), 'where': where(k)})
    tally = collections.Counter(r['level'] for r in rows)
    metrows = [r for r in rows if r['met'] > 0]
    out['categories'][name] = {
        'total': len(rows), 'by_level': {o: tally.get(o, 0) for o in ORDER},
        'met_total': len(metrows), 'met_by_level': {o: sum(1 for r in metrows if r['level'] == o) for o in ORDER},
        'items': sorted(rows, key=lambda r: (-r['met'], r['key'])),
    }

base_p = ROOT / 'tools' / 'coverage' / 'baseline.json'
shares = {n: {'total': c['total'], 'by_level': c['by_level'], 'met_total': c['met_total'], 'met_by_level': c['met_by_level']} for n, c in out['categories'].items()}
if '--baseline' in sys.argv:
    base_p.write_text(json.dumps(shares, indent=1) + '\n')
base = json.loads(base_p.read_text()) if base_p.exists() else shares
out['before'] = base
(ROOT / 'data' / 'coverage.json').write_text(json.dumps(out, indent=1) + '\n')


def pct(a, b):
    return f'{a}/{b} ({100 * a / b:.0f}%)' if b else '-'


L = []
L.append('# Coverage: what never appears in a frame compared with the game\n')
L.append('Made by `tools/coverage/run.sh` + `tools/coverage/report.py` (do not edit by hand; the numbers are in '
         '`data/coverage.json`). Every gated comparison is replayed with a recorder that only counts '
         '(`WF_COVERAGE`, `engine/crates/wf-view/src/coverage.rs`), and what it saw is set against the game\'s content.\n')
L.append('An item\'s level is the strongest comparison it appears in:\n')
L.append('* **picture** - in a picture a gate compares with the game\'s: the pixel lock\'s frames, the menu scenarios, the link recordings;')
L.append('* **state** - in a replay a gate compares with the game\'s state every frame, without a picture of it: the segment gate\'s '
         'matched frames, the three whole runs (`tools/wholerun/check.sh`), the battle lockstep traces;')
L.append('* **audit** - only in the by-hand opponent / skill audits in `work/` (every frame against the game, not gated here);')
L.append('* **board** - only on the picture board, in a sampled frame that is not locked (it differs, or its segment has another differing frame);')
L.append('* **never** - in no frame compared with the game.\n')
L.append('"Met" = present in at least one logic iteration of the three recorded whole playthroughs; the lists are ordered by that count '
         '(iterations present), the measure of how often a player meets a thing.\n')
L.append(f"Levels in this report: {', '.join(sorted(levels_present))}.\n")
L.append('## Summary\n')
L.append('Share with a **picture** comparison, before -> after this pass (of everything in the content, and of what the whole runs meet):\n')
L.append('| Category | In the content | Picture before | Picture now | Picture or state now | Met in the runs | Picture, of met: before | now | Never compared |')
L.append('|---|---|---|---|---|---|---|---|---|')
for n, c in out['categories'].items():
    b = base.get(n, shares[n])
    ps = c['by_level']['picture'] + c['by_level']['state']
    L.append(f"| {n} | {c['total']} | {pct(b['by_level']['picture'], b['total'])} | {pct(c['by_level']['picture'], c['total'])} | {pct(ps, c['total'])} | "
             f"{c['met_total']} | {pct(b['met_by_level']['picture'], b['met_total'])} | {pct(c['met_by_level']['picture'], c['met_total'])} | {c['by_level']['never']} |")
L.append('')
extra = ROOT / 'tools' / 'coverage' / 'notes.md'
if extra.exists():
    L.append(extra.read_text().rstrip() + '\n')
# the scenes this pass put on the picture board (the board's per-frame results; notes: tools/coverage/scenes.json)
board_p = ROOT / 'work' / 'coverage' / 'board.json'
notes = json.loads((ROOT / 'tools' / 'coverage' / 'scenes.json').read_text())
lock = json.loads((ROOT / 'data' / 'segments' / 'pixels_coverage.json').read_text())['segments'] if (ROOT / 'data' / 'segments' / 'pixels_coverage.json').exists() else {}
if board_p.exists():
    new = [s for s in json.loads(board_p.read_text()) if s['segment'].startswith(('cov_', 'cov2_', 'tour_'))]
    fr, ex = sum(s['frames'] for s in new), sum(s['exact'] for s in new)
    L.append('## New on the picture board in this pass\n')
    L.append(f"{len(new)} scenes, {ex:,} of {fr:,} sampled frames exact; the exact frames are locked in `data/segments/pixels_coverage.json` "
             f"({sum(len(v) for v in lock.values()):,} frames of {len(lock)} scenes, read by the pixel gate next to the main lock) - "
             "**to add to `data/segments/pixels.json` when it is re-made** (their screen dumps are in `work/superbuu/pix/fb/`).\n")
    L.append('* `cov_*` (38) and `cov2_*` (43): cut from the three whole runs at the row where an uncovered thing first occurs (`tools/coverage/cuts.py`, '
             'a picture every 10th frame; `source` names the run and row); each is also on the state gate (`expect.matched`).')
    L.append('* `tour_*` (120 of 121): the field tours, on the state gate before but never dumped (`tools/coverage/dump.py`, every 10th frame).\n')
    out['new_scenes'] = []
    L.append('### Known differences in the new scenes\n')
    L.append('Every scene with a differing sampled frame: its first one (video frame, the board\'s class, pixels), how many differ, and what it is where it was looked at.\n')
    L.append('| Scene | Exact / sampled | First differing frame | Differing frames by class | What |')
    L.append('|---|---|---|---|---|')
    for s in sorted(new, key=lambda s: (s['segment'].startswith('tour_'), -(s['frames'] - s['exact']))):
        bad = [(f, c, n) for f, c, n in s['results'] if n > 0]
        cl = collections.Counter(c.split(' [')[0] for _, c, _ in bad)
        out['new_scenes'].append({'segment': s['segment'], 'frames': s['frames'], 'exact': s['exact'], 'first': bad[0] if bad else None, 'classes': dict(cl), 'note': notes.get(s['segment'], '') if isinstance(notes.get(s['segment'], ''), str) else ''})
        if not bad and s['segment'] not in notes:
            continue
        first = f"f{bad[0][0]} {bad[0][1].split(' [')[0]} ({bad[0][2]} px)" if bad else '-'
        L.append(f"| {s['segment']} | {s['exact']} / {s['frames']} | {first} | {', '.join(f'{k} {v}' for k, v in cl.most_common())} | {notes.get(s['segment'], '')} |")
    L.append('')
    L.append('### Tried and not on the board\n')
    L.append('| What | Why |')
    L.append('|---|---|')
    for what, why in notes.get('_not_recorded', []):
        L.append(f'| {what} | {why} |')
    L.append('')
    (ROOT / 'data' / 'coverage.json').write_text(json.dumps(out, indent=1) + '\n')
L.append('## Still without a picture comparison, most met first\n')
L.append('Items a player meets in the whole runs that no gated picture shows (level in brackets; where it can be found).\n')
for n, c in out['categories'].items():
    rows = [r for r in c['items'] if r['level'] != 'picture' and r['met'] > 0]
    if not rows:
        continue
    L.append(f"### {n} ({len(rows)})\n")
    L.append('| Item | Met (iterations) | Level | Seen in |')
    L.append('|---|---|---|---|')
    for r in rows[:60]:
        L.append(f"| {r['key']} {r['label']} | {r['met']:,} | {r['level']} | {r['where']} |")
    if len(rows) > 60:
        L.append(f"| ... {len(rows) - 60} more in data/coverage.json | | | |")
    L.append('')
L.append('## Not met in the three whole runs and never in a picture\n')
L.append('Content the recorded playthroughs do not reach (optional areas, other characters\' skills, unused data); counts only, the lists are in `data/coverage.json`.\n')
L.append('| Category | Not met, no picture | of them state / audit / board / never | Examples |')
L.append('|---|---|---|---|')
for n, c in out['categories'].items():
    rows = [r for r in c['items'] if r['level'] != 'picture' and r['met'] == 0]
    if rows:
        t = collections.Counter(r['level'] for r in rows)
        ex = ', '.join(f"{r['key']} {r['label']}".strip() for r in rows[:5])
        L.append(f"| {n} | {len(rows)} | {t.get('state', 0)} / {t.get('audit', 0)} / {t.get('board', 0)} / {t.get('never', 0)} | {ex} |")
L.append('')
(ROOT / 'engine' / 'docs' / 'coverage.md').write_text('\n'.join(L))
for n, c in out['categories'].items():
    print(f"{n:28} total {c['total']:4}  {c['by_level']}  met {c['met_total']} {c['met_by_level']}")
