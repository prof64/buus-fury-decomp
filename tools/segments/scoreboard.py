"""Campaign-wide parity scoreboard: the REAL playthrough (tools/segments/realrun.py) cut into segments at every
importable area entry, replayed on wf-core.

usage: scoreboard.py cut [JOBS] [ONLY ...]   (once / when the real run changes; needs the local states under work/)
           work/real/runs/NNN_<name>/ (realrun.py runs) -> re/buu/ship/rec_segments.py -> work/real/segs/NNN_<name>/
           + init.snd_phase (snd_phase.py); writes work/real/cuts.json (per stretch: frames, cut frames, segments)
       scoreboard.py [JOBS]                  (after every engine change: one command)
           replays every work/real/segs/* directory (cargo test -p wf-import --test segments, SEGMENT_STRICT=0, JOBS
           directories at a time), writes engine/docs/scoreboard.md and data/realrun/scoreboard.json
The cuts stay local (work/); the tool, the doc and the summary are what is committed. Deterministic: the same tree
gives the same board (wall-clock times are not written)."""
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REAL = ROOT / 'work' / 'real'
DATA = ROOT / 'data' / 'realrun'
PY = str(ROOT / '.venv' / 'bin' / 'python')
# (its own target dir: seven agents share engine/target and cargo's lock on it makes every build queue)
CARGO = ['cargo', 'test', '--release', '-p', 'wf-import', '--test', 'segments', '--target-dir',
         str(ROOT / 'engine' / 'target-route')]
LINE = re.compile(r'^segment (\S+): (\d+) video frames \((\d+) logic\), matched (\d+) / (\d+) \| first divergence (.*?) \| import held')

# first-divergence text -> category (first match wins) and the agent that owns that kind of work (segments.md)
CATS = [   # the first divergence's kind (wf_import::segment's report: "<kind>: ...") -> category
    ('importer', ('import', 'unsupported')),
    ('sound', ('snd:',)),
    ('menu timing', ('menu', 'redraw', 'lag')),
    ('sprite memory', ('tile', 'vram', 'oam')),
    ('RNG', ('rng:',)),
    ('script / text', ('text:', 'script', 'flag', 'event', 'depth')),
    ('screen fade', ('fade:',)),
    ('camera', ('cam:',)),
    ('animation', ('anim:', 'cel', 'pose')),
    ('position / facing', ('pos:', 'dir:', 'hp:')),
    ('object set', ('missing:', 'extra:')),
]
OWNER = {'menu timing': 'Earth agent (redraw / mixer timing)', 'sound': 'Earth agent (sound timing) / audio',
         'importer': 'importer (wf-import)', 'sprite memory': 'chapter agent (tile allocator order)',
         'RNG': 'chapter agent (who draws first: enemy AI / hits)', 'script / text': 'cutscene / script-host agent',
         'screen fade': 'cutscene / script-host agent', 'camera': 'chapter agent (camera scroll)',
         'animation': 'chapter agent', 'position / facing': 'chapter agent (enemy AI / knockback)',
         'object set': 'chapter agent (spawns / removal)', 'other': '-'}
CHAPTER_OWNER = {'opening': 'Earth agent (title, New Game, Other World)', 'earth': 'Earth agent', 'ship': 'ship agent', 'fat': 'dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)',
                 'janemba': 'side-content agent', 'superbuu': 'Super Buu agent'}


# what is known about a segment's first divergence (investigated; shown with it on the board)
NOTES = {
    'r069_168_14-1': 'Switch Character closed at the 14-1 save point (Vegeta -> Goku): the world restore\'s black lasts '
                     '26 frames, sndsim::reload_frames_with predicts 25 (end 6,961,842 cycles, 60,558 short of the '
                     '26th frame)',
    'r067_136_14-1': 'Switch Character at 14-1 (Goku -> Vegeta): game 25 black frames, predicted 24 (46,986 short); '
                     'the same save point\'s Trunks <-> Goten switches (r017_037) predict right with margins 86 k / 50 k',
    'r044_015_18-1': 'Switch Character at 18-1 (Goku -> Vegeta): predicted 23 black frames, the game 22 (over by '
                     '>= 21,578 cycles)',
    'r064_017_21-14': 'Switch Character at 21-14 (Vegeta -> Goku, 4 figures): predicted 15, the game 14 (over by '
                      '>= 99 k cycles: CHARSEL_CLOSE_SPRITE x 4?)',
    'rE2__090_7-3': 'not the Earth agent\'s e_7_3_1: the real run\'s Earth chapter cut at its last importable entry, '
                    'Papaya Island 7-3 (file work/real/segs/E2_earth/rE2__090_7-3.json, state states/rE2__090_7-3.ss, '
                    'key frame 108,563 of the Earth keys): 52,416 frames, the Earth keys\' last 51,940 (no importable entry '
                    'after it) and ~480 of the ship route\'s up to its first entry; first divergence at its frame 6,658: actor #42 '
                    '(sprite 222) raw 1 cel 0 vs the game\'s cel 1, tick 1776 vs 1777 - an NPC idle animation one tick '
                    'behind after ~1,800 frames',
    'r002_006_4-1': 'West City -> the Bandana hunt (fixed: s509 a frame later 1,763; on-defeat prop show 4,178). Now '
                    'at 10,623: a heal menu closed on the 3D world map 0-55 - the reload estimate predicts 27 black '
                    'frames (margin 58,318 cycles), the game takes 28 (its camera step lands in the 28th): reload '
                    'model on a 3D map',
    'r038_002_18-2': 'a heal menu closed in 18-2: predicted 21 black frames 208 cycles before the 22nd',
    'r047_002_18-13': 'heal menu / Switch Character in 18-13: one frame short',
}


def owner_of(ch, cat):
    o = OWNER.get(cat, '-')
    return CHAPTER_OWNER.get(ch, ch) if o.startswith('chapter agent') else o


def category(text):
    t = text.lower()
    for name, keys in CATS:
        if any(k in t for k in keys):
            return name
    return 'other'


def cut_one(d):
    seg = REAL / 'segs' / d.name
    if seg.exists():
        for f in seg.glob('*.json'):
            f.unlink()
    r = subprocess.run([PY, str(ROOT / 're/buu/ship/rec_segments.py'), str(d), str(seg), '--prefix', 'r' + d.name[:3]],
                       capture_output=True, text=True, cwd=ROOT)
    files = sorted(seg.glob('*.json')) if seg.exists() else []
    if files:
        subprocess.run([PY, str(ROOT / 'tools/segments/snd_phase.py')] + [str(f) for f in files],
                       capture_output=True, text=True, cwd=ROOT)
    cut = 0
    for ln in r.stdout.splitlines():
        m = re.match(r'\S+\.json (\d+) frames', ln)
        if m:
            cut += int(m.group(1))
    tail = (r.stdout + r.stderr).strip().splitlines()[-1:] if not files else []
    return d.name, len(files), cut, tail


def run_dirs():
    """the run dirs in playing order: E1_opening, E2_earth, then 000_ .. (the real run's stretches)"""
    return sorted((d for d in (REAL / 'runs').iterdir() if d.is_dir()), key=lambda d: (not d.name.startswith('E'), d.name))


def do_cut(jobs, only):
    subprocess.run([PY, str(ROOT / 'tools/segments/realrun.py'), 'runs', str(REAL / 'runs')], check=True, cwd=ROOT)
    man = {f'{m["n"]:03d}_{m["name"]}': m for m in json.loads((DATA / 'manifest.json').read_text())}
    sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
    sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
    sys.path.insert(0, str(ROOT / 'tools' / 'field'))
    import realrun
    man.update(realrun.pre_manifest())
    dirs = [d for d in run_dirs() if not only or d.name[:3] in only or d.name[:2] in only]
    p = REAL / 'cuts.json'
    cuts = json.loads(p.read_text()) if p.exists() else {}
    with ThreadPoolExecutor(jobs) as ex:
        for name, n, cut, tail in ex.map(cut_one, dirs):
            cuts[name] = {'chapter': man[name]['chapter'], 'frames': man[name]['frames'], 'cut_frames': cut, 'segments': n}
            print(name, n, 'segments', cut, '/', man[name]['frames'], 'frames', *tail, flush=True)
            p.write_text(json.dumps(cuts, indent=0, sort_keys=True))


def extend_one(job):
    """stretch j's last segment ends where j's key log ends (a stretch boundary is not a cut): re-record it from
    its own start state over j's tail + the following stretches' keys up to the next importable entry anywhere
    (rec_segments --at-start --max 1), overwriting it in place"""
    j, last, tail_keys = job
    run = REAL / 'ext' / j
    run.mkdir(parents=True, exist_ok=True)
    seg = REAL / 'segs' / j
    import shutil
    shutil.copyfile(seg / 'states' / (last.stem + '.ss'), run / 'start.ss')
    (run / 'keys.json').write_text(json.dumps({'from': None, 'keys': tail_keys}))
    n = int(re.search(r'_(\d{3})_', last.stem).group(1))
    tmp = REAL / 'ext' / (j + '_out')
    if tmp.exists():
        shutil.rmtree(tmp)
    subprocess.run([PY, str(ROOT / 're/buu/ship/rec_segments.py'), str(run), str(tmp), '--prefix', 'r' + j[:3],
                    '--first', str(n), '--max', '1', '--at-start'], capture_output=True, text=True, cwd=ROOT)
    new = sorted(tmp.glob('*.json'))
    if len(new) != 1 or new[0].name != last.name:
        return j, f'extension failed: {[x.name for x in new]}'
    subprocess.run([PY, str(ROOT / 'tools/segments/snd_phase.py'), str(new[0])], capture_output=True, text=True, cwd=ROOT)
    # keep the stretch's own source (run dir + key frame of the cut): what `extend` reads the next time (idempotent)
    # and what snd_phase.py derives the same phase from
    old_src = re.search(r'"source":\{[^}]*\}', open(last).readline()).group(0)
    s = new[0].read_text()
    nl = s.index('\n')
    last.write_text(re.sub(r'"source":\{[^}]*\}', lambda m: old_src, s[:nl], count=1) + s[nl:])
    new[0].unlink()
    return j, f'{last.name} extended over {len(tail_keys)} frames'


def do_extend(jobs):
    runs = run_dirs()
    keys = {d.name: json.loads((d / 'keys.json').read_text())['keys'] for d in runs}
    names = [d.name for d in runs]
    work = []
    for i, j in enumerate(names):
        segs = sorted((REAL / 'segs' / j).glob('*.json'))
        if not segs or i + 1 == len(names):
            continue
        last = segs[-1]
        f0 = int(re.search(r'"frame":(\d+)', open(last).readline()).group(1))
        tail = keys[j][f0:]
        for k in names[i + 1:]:
            tail += keys[k]
            if any((REAL / 'segs' / k).glob('*.json')):
                break
        work.append((j, last, tail))
    with ThreadPoolExecutor(jobs) as ex:
        for j, msg in ex.map(extend_one, work):
            print(j, msg, flush=True)


EXE = None   # the segments test binary, built once per board (one snapshot of the tree for every directory)


def build():
    global EXE
    r = subprocess.run(CARGO + ['--no-run', '--message-format=json'], capture_output=True, text=True,
                       cwd=ROOT / 'engine')
    for ln in r.stdout.splitlines():
        try:
            j = json.loads(ln)
        except ValueError:
            continue
        if j.get('reason') == 'compiler-artifact' and j.get('executable') and j['target']['name'] == 'segments':
            EXE = j['executable']
    if not EXE:
        raise SystemExit('the segments test does not build:\n' + r.stderr[-3000:])


def replay_one(d):
    env = dict(os.environ, SEGMENT_DIR=str(d), SEGMENT_STRICT='0')
    r = subprocess.run([EXE, '--nocapture'], capture_output=True,
                       text=True, cwd=ROOT / 'engine', env=env)
    rows = []
    for ln in r.stderr.splitlines():
        m = LINE.match(ln)
        if m:
            rows.append({'segment': m.group(1), 'frames': int(m.group(2)), 'matched': int(m.group(4)),
                         'first': m.group(6)})
    return d.name, rows, r.returncode


def mid_section(top=5):
    """the every-N board's divergences (data/realrun/scoreboard_mid.json, `scoreboard.py mid`) per chapter, with
    their owners: each tile divergence is a separate cause, weighed by the frames it leaves unverified up to the next
    cut"""
    p = DATA / 'scoreboard_mid.json'
    if not p.exists():
        return []
    m = json.loads(p.read_text())
    n = m['tile']
    wg = m['total'].get('whole_game')
    L = ['', '## Where long segments really diverge (every-N board)', '',
         f'From `engine/docs/scoreboard-mid.md` (tree `{m["tree"]}`, tiles of {n:,} frames'
         + (f'; the whole run {100.0 * wg["verified"] / wg["frames"]:.1f}% verified exact' if wg else '') + '). Each',
         'divergence below is where a tile (a cut inside a long segment) first differs; "lost" = its frames up to the next',
         'cut. A divergence at a tile\'s frame 0 is usually the any-frame import, not the game rule - say so to the importer',
         'agent.', '']
    by, imp = {}, {}
    for sg in m['segments']:
        for sp in sg['spots']:
            k, cat, t = sp[:3]
            if cat == 'refused':
                continue
            k0, k1 = sp[3:5] if len(sp) > 3 else (k // n * n, min(sg['frames'], (k // n + 1) * n))
            lost = k1 - k
            if k == k0:
                # at the tile's own first frame: what the import rebuilt differs from RAM (not a game rule)
                kind = re.sub(r'#\d+ |[-\d.]+', '', t.split('"', 2)[1] if '"' in t else t)[:40].strip(' :,()')
                w = imp.setdefault((cat, kind), [0, 0, set()])
                w[0] += 1
                w[1] += lost
                w[2].add(sg['chapter'])
                continue
            by.setdefault(sg['chapter'], []).append((lost, sg['segment'], k, cat, t))
    for ch in ('opening', 'earth', 'ship', 'fat', 'janemba', 'superbuu'):
        if ch not in by:
            continue
        items = sorted(by[ch], key=lambda x: (-x[0], x[1], x[2]))
        L.append(f"**{ch}** ({CHAPTER_OWNER.get(ch, '')}) - {len(items)} divergences inside tiles, "
                 f"{sum(x[0] for x in items):,} frames")
        L.append('')
        for lost, seg, k, cat, t in items[:top]:
            o = owner_of(ch, cat)
            L.append(f"* `{seg}` frame {k:,} ({cat}{'' if o == CHAPTER_OWNER.get(ch) else ', ' + o}): {lost:,} lost - {t[:120]}")
        L.append('')
    if imp:
        L += ['**importer** (any-frame import) - tiles that differ at their first frame, by kind:', '']
        for (cat, kind), (c, f, chs) in sorted(imp.items(), key=lambda x: -x[1][1])[:10]:
            L.append(f"* {cat} `{kind}`: {c} tiles, {f:,} frames ({', '.join(sorted(chs))})")
        L.append('')
    return L


def board(jobs):
    build()
    cuts = json.loads((REAL / 'cuts.json').read_text())
    dirs = [d for d in sorted((REAL / 'segs').iterdir()) if d.is_dir() and any(d.glob('*.json'))]
    res = {}
    with ThreadPoolExecutor(jobs) as ex:
        for name, rows, rc in ex.map(replay_one, dirs):
            if not rows and rc != 0:
                raise SystemExit(f'{name}: the replay did not run (build error? another agent mid-edit): rc {rc}')
            res[name] = rows
            print(name, len(rows), 'segments', sum(r['matched'] >= r['frames'] for r in rows), 'exact', flush=True)
    # (the per-segment results: what `mid cut` picks its long non-exact segments from)
    (REAL / 'board_rows.json').write_text(json.dumps(res, indent=0, sort_keys=True))
    chapters = {}
    bad = []
    for name in sorted(cuts):
        c = cuts[name]
        ch = chapters.setdefault(c['chapter'], {'stretches': 0, 'run_frames': 0, 'cut_frames': 0, 'segments': 0,
                                                'exact': 0, 'frames': 0, 'matched': 0, 'cats': {}})
        ch['stretches'] += 1
        ch['run_frames'] += c['frames']
        for r in res.get(name, []):
            ch['segments'] += 1
            ch['frames'] += r['frames']
            ch['cut_frames'] += r['frames']
            ch['matched'] += min(r['matched'], r['frames'])
            if r['matched'] >= r['frames']:
                ch['exact'] += 1
            else:
                cat = category(r['first'])
                # a fade off by a frame after a menu / Switch Character closed = the world restore's length
                if cat == 'screen fade' and 'predict' in NOTES.get(r['segment'], ''):
                    cat = 'menu timing'
                ch['cats'][cat] = ch['cats'].get(cat, 0) + 1
                bad.append((c['chapter'], name, r, cat))
    tot = {k: sum(c[k] for c in chapters.values()) for k in ('stretches', 'run_frames', 'cut_frames', 'segments', 'exact', 'frames', 'matched')}
    head = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT).stdout.strip()
    dirty = bool(subprocess.run(['git', 'status', '--porcelain', 'engine/crates'], capture_output=True, text=True, cwd=ROOT).stdout.strip())
    L = ['# Parity scoreboard: the real playthrough on wf-core', '',
         f'Tree: `{head}`{" + uncommitted engine changes" if dirty else ""}. Regenerate: `.venv/bin/python tools/segments/scoreboard.py`',
         '(the cuts: `scoreboard.py cut`, local under `work/real/`; the run itself: `tools/segments/realrun.py`, §"How to replay',
         'the real run" in `engine/docs/segments.md`). A segment = the real game from one importable area entry (an exit',
         'arrival with nothing running) to the next; an entry by a script warp / in a cutscene is not a cut - its frames',
         'stay in the segment before it, and a segment runs on across the boundaries of the run\'s key logs (`scoreboard.py',
         'extend`). "not cut" = frames in no segment (the run\'s start up to its first importable entry) - a chapter\'s',
         'frames are counted where its segments start.', '',
         '| chapter | owner | run frames | not cut | segments | exact | frames | matched | % |', '|---|---|---|---|---|---|---|---|---|']
    order = [c for c in ('opening', 'earth', 'ship', 'fat', 'janemba', 'superbuu') if c in chapters]
    for k in order + ['**all**']:
        c = tot if k == '**all**' else chapters[k]
        pct = 100.0 * c['matched'] / c['frames'] if c['frames'] else 0.0
        L.append(f"| {k} | {CHAPTER_OWNER.get(k, '')} | {c['run_frames']:,} | {c['run_frames'] - c['cut_frames']:,} | {c['segments']} | "
                 f"{c['exact']} | {c['frames']:,} | {c['matched']:,} | {pct:.1f} |")
    # the end-to-end line: the app from the reset with the run's keys, no import anywhere (tools/wholerun/board.py)
    wr = ROOT / 'data' / 'realrun' / 'wholerun.json'
    if wr.exists():
        w = json.loads(wr.read_text())
        for m in w.get('modes', []):
            L.append(f"\n**Whole run, no imports ({m['lag']} lag): exact to frame {m['exact_to']:,}** of {m['rows']:,} recorded "
                     f"({m['exact_stretch']}; first divergence: {m['first']}). Tree `{w.get('tree', '?')}`, `tools/wholerun/board.py`.")
        if w.get('note'):
            L.append(w['note'])
    L += ['', '## Non-exact segments by category', '', '| category | owner | ' + ' | '.join(order) + ' | all |', '|---|---|' + '---|' * (len(order) + 1)]
    names = [n for n, _ in CATS] + ['other']
    for cat in names:
        row = [chapters[c]['cats'].get(cat, 0) for c in order]
        if sum(row):
            L.append(f"| {cat} | {OWNER[cat]} | " + ' | '.join(str(x) for x in row) + f' | {sum(row)} |')
    L += ['', '## Top items per owner', '', 'The five biggest losses each owner can take (unmatched frames; the full list below).', '']
    by = {}
    for c, name, r, cat in bad:
        by.setdefault(owner_of(c, cat), []).append((r['frames'] - r['matched'], r, cat, name))
    for o in sorted(by, key=lambda o: -sum(x[0] for x in by[o])):
        items = sorted(by[o], key=lambda x: -x[0])
        L.append(f"**{o}** - {len(items)} segments, {sum(x[0] for x in items):,} frames lost")
        L.append('')
        for lost, r, cat, name in items[:5]:
            note = NOTES.get(r['segment'])
            L.append(f"* `{r['segment']}` ({name}, {cat}): {lost:,} lost - {r['first'][:120]}" + (f' **Known:** {note}' if note else ''))
        L.append('')
    L += mid_section()
    L += ['', '## Biggest losses (unmatched frames)', '', '| segment | stretch | category | lost | first divergence |',
          '|---|---|---|---|---|']
    for c, name, r, cat in sorted(bad, key=lambda b: -(b[2]['frames'] - b[2]['matched']))[:15]:
        L.append(f"| `{r['segment']}` | {name} | {cat} | {r['frames'] - r['matched']:,} | {r['first'][:110]} |")
    # the same kind of first divergence on the same sprite (or the same text / sound): likely one cause
    groups = {}
    for c, name, r, cat in bad:
        m = re.search(r'"(\w+): (?:#\d+ )?(sprite \d+)?', r['first'])
        key = (m.group(1) + (' ' + m.group(2) if m.group(2) else '')) if m else cat
        if m and m.group(1) == 'snd':
            key = 'snd ' + (re.search(r'play \d+', r['first']) or re.search(r'snd', r['first'])).group(0)
        elif m and m.group(1) in ('text', 'fade'):
            key = re.search(r'"([^"]{0,60})', r['first']).group(1)
        groups.setdefault(key, []).append(r['segment'])
    rec = sorted(((k, v) for k, v in groups.items() if len(v) >= 3), key=lambda kv: (-len(kv[1]), kv[0]))
    if rec:
        L += ['', '## Recurring first divergences (3+ segments)', '']
        for k, v in rec:
            L.append(f"* {len(v)} x `{k}`: " + ', '.join(f'`{x}`' for x in v[:8]) + (' ...' if len(v) > 8 else ''))
    L += ['', '## Every non-exact segment', '', 'segment (stretch): matched / frames, category - first divergence', '']
    for ch in order:
        L.append(f'### {ch}')
        L.append('')
        for c, name, r, cat in bad:
            if c == ch:
                note = NOTES.get(r['segment'])
                L.append(f"* `{r['segment']}` ({name}): {r['matched']:,} / {r['frames']:,}, **{cat}** - {r['first'][:160]}"
                         + (f' **Known:** {note}' if note else ''))
        L.append('')
    (ROOT / 'engine' / 'docs' / 'scoreboard.md').write_text('\n'.join(L))
    (DATA / 'scoreboard.json').write_text(json.dumps({'tree': head, 'dirty': dirty, 'total': tot, 'chapters': chapters}, indent=1, sort_keys=True))
    print('all:', tot)


# --- every-N-frames cuts (any-frame import): where a long segment really diverges ----------------------------------
# `mid cut N [JOBS] [SEG ...]`: every area-entry segment that is not exact and loses more than N frames (or the named
# ones) is re-recorded from its start state and cut into tiles of N frames, one at its frame N, 2N, ... (the tile
# at 0 is the segment itself) -> work/real/mid/<stretch>/<segment>+<frame>.json (init.snd_phase from the segment's).
# `mid [JOBS]`: replays the tiles -> engine/docs/scoreboard-mid.md + data/realrun/scoreboard_mid.json: per frame of
# those segments, verified exact (the segment's own matched prefix or a tile's), diverged (a tile after its first
# divergence) or not covered (a refused tile: the importer cannot start there yet).
MID = REAL / 'mid'
TILE = re.compile(r'^(.+)\+(\d{6})$')


def mid_cut_one(job):
    d, seg, n = job
    sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
    sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
    sys.path.insert(0, str(ROOT / 'tools' / 'field'))
    import segment as sg
    from route import start_emu
    head, frames = sg.load_doc(REAL / 'segs' / d / f'{seg}.json')
    ph = head['init'].get('snd_phase')
    e = start_emu(str(REAL / 'segs' / d / 'states' / f'{seg}.ss'))
    out = MID / d
    out.mkdir(parents=True, exist_ok=True)
    for j in range(n):
        e.set_keys(frames[j]['keys'])
        e.run(1)
    k, made, bad = n, 0, None
    while k + n // 4 < len(frames):
        m = min(n, len(frames) - k)
        # a tile that would start inside a menu screen (no leader: the field is torn down while the page shows) starts
        # at the first frame the field is back and lit (the import cannot rebuild a saved world; the skipped frames
        # stay not covered - dungeon agent A, scoreboard-mid "on-enter script is still blocked")
        if not frames[k].get('p'):
            # (a load - the area or the leader's place changed across the gap - starts at the first frame with a
            # leader: the importer's area-entry point; a menu's return waits for the lit screen)
            b = next((f for f in reversed(frames[:k]) if f.get('p')), None)
            nxt = next((f for f in frames[k:k + m] if f.get('p')), None)
            load = b is None or nxt is None or b.get('area') != nxt.get('area') or b['p'][:2] != nxt['p'][:2]
            j = k
            while j < k + m and not (frames[j].get('p') and (load or (frames[j].get('io') or [1] * 6)[5] != 0)):
                e.set_keys(frames[j]['keys'])
                e.run(1)
                j += 1
            # (a load: the state at the start is the RAM before the frame - the leader made in it is there one later)
            if load and j < k + m:
                e.set_keys(frames[j]['keys'])
                e.run(1)
                j += 1
            if j >= k + m:
                k += m
                continue
            m -= j - k
            k = j
        init, objs, rows = sg.record(e, lambda j, _e: frames[k + j]['keys'], m)
        for j, r in enumerate(rows):
            f = frames[k + j]
            diff = [x for x in ('lf', 'rng', 'area', 'p') if f.get(x) != r.get(x)]
            if diff and bad is None:
                bad = f'{seg}: re-recorded frame {k + j} differs ({diff})'
        if ph is not None:
            # one video frame = 280,896 cycles = 16 sound periods + 12,096 (snd_phase.py)
            init = {'snd_phase': (ph - k * 12096) % 16800, **init}
        name = f'{seg}+{k:06d}'
        src = {'kind': 'tile', 'of': seg, 'at': k}
        sg.OUT = out
        sg.save(name, {'kind': 'segment', 'name': name, 'source': src, 'init': init, 'objs': objs, 'frames': rows})
        made += 1
        k += m
    return seg, made, bad


def do_mid_cut(n, jobs, only):
    from concurrent.futures import ProcessPoolExecutor
    res = json.loads((REAL / 'board_rows.json').read_text())
    work = []
    for d, rows in sorted(res.items()):
        for r in rows:
            if only and r['segment'] not in only:
                continue
            if not only and r['frames'] - r['matched'] <= n:
                continue
            work.append((d, r['segment'], n))
    for d, seg, _ in work:
        for p in (MID / d).glob(f'{seg}+*.json'):
            p.unlink()
    with ProcessPoolExecutor(jobs) as ex:
        for seg, made, bad in ex.map(mid_cut_one, work):
            print(seg, made, 'tiles', bad or '', flush=True)
    (MID / 'tile.json').write_text(json.dumps({'n': n}))


def replay_tile(p, limit=600):
    """one tile on its own (a tile whose replay never ends - an engine loop - is reported, not waited on)"""
    env = dict(os.environ, SEGMENT_DIR=str(p.parent), SEGMENT=p.stem, SEGMENT_STRICT='0')
    n = max(1, sum(1 for ln in p.open() if ln.startswith('{"keys"')))
    try:
        r = subprocess.run([EXE, '--nocapture'], capture_output=True, text=True, cwd=ROOT / 'engine', env=env,
                           timeout=limit)
    except subprocess.TimeoutExpired:
        return {'segment': p.stem, 'frames': n, 'matched': 0, 'first': f'Some((0, "import: replay hung (> {limit} s)"))'}
    for ln in r.stderr.splitlines():
        m = LINE.match(ln)
        if m:
            return {'segment': m.group(1), 'frames': int(m.group(2)), 'matched': int(m.group(4)), 'first': m.group(6)}
    tail = (r.stderr.strip().splitlines() or ['no output'])[-1][:100].replace('"', "'")
    return {'segment': p.stem, 'frames': n, 'matched': 0, 'first': f'Some((0, "import: replay failed: {tail}"))'}


def playthrough_credit():
    """frames of E1_opening before its first segment that the playthrough test verifies (0 if the keys differ):
    from the recording's first compared row (after its `skip` intro frames) to the first cut"""
    sys.path.insert(0, str(ROOT / 'tools' / 'segments'))
    sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
    sys.path.insert(0, str(ROOT / 'tools' / 'field'))
    import realrun
    import segment as sg
    from route import intro_keys
    segs = sorted((REAL / 'segs' / 'E1_opening').glob('*.json'))
    if not segs:
        return 0
    f0 = int(re.search(r'"frame":(\d+)', open(segs[0]).readline()).group(1))
    head, rows = sg.load_doc(ROOT / 'data' / 'story_playthrough' / 'opening.json')
    keys = [intro_keys(i) for i in range(head['skip'])] + [r['keys'] for r in rows]
    if realrun.pre_keys()['start'][:f0] != keys[:f0]:
        return 0
    return max(0, f0 - head['skip'])


def mid_board(jobs):
    build()
    res = json.loads((REAL / 'board_rows.json').read_text())
    cuts = json.loads((REAL / 'cuts.json').read_text())
    n = json.loads((MID / 'tile.json').read_text())['n']
    paths = sorted(p for d in MID.iterdir() if d.is_dir() for p in d.glob('*.json'))
    tiles = {}
    with ThreadPoolExecutor(jobs) as ex:
        for r in ex.map(replay_tile, paths):
            m = TILE.match(r['segment'])
            tiles.setdefault(m.group(1), []).append((int(m.group(2)), r))
            print(r['segment'], r['matched'], '/', r['frames'], r['first'][:80], flush=True)
    segs, chapters = [], {}
    for d, rows in sorted(res.items()):
        for r in rows:
            if r['segment'] not in tiles:
                continue
            ts = sorted(tiles[r['segment']], key=lambda t: t[0])
            # per frame: 2 verified (the segment's own exact prefix, or a tile's), 1 not covered (a refused tile),
            # 0 diverged
            nf = r['frames']
            st = bytearray(nf)
            st[:min(r['matched'], nf)] = b'\x02' * min(r['matched'], nf)
            spots = []
            for k, t in ts:
                e = min(nf, k + t['frames'])
                if t['first'].startswith('Some((0, "import'):
                    for i in range(k, e):
                        if st[i] == 0:
                            st[i] = 1
                    spots.append((k, 'refused', t['first'], k, e))
                    continue
                st[k:min(e, k + t['matched'])] = b'\x02' * (min(e, k + t['matched']) - k)
                if t['matched'] < t['frames']:
                    # (+ the tile's start and end: tiles need not sit on multiples of N - one that would start
                    # inside a menu starts when the field is back)
                    spots.append((k + t['matched'], category(t['first']), t['first'], k, e))
            ok, unc = st.count(2), st.count(1)
            div = nf - ok - unc
            s = {'segment': r['segment'], 'stretch': d, 'chapter': cuts.get(d, {}).get('chapter', '?'),
                 'frames': r['frames'], 'own': r['matched'], 'own_first': r['first'], 'verified': ok, 'diverged': div,
                 'uncovered': unc, 'tiles': len(ts), 'exact_tiles': sum(t['matched'] >= t['frames'] for _, t in ts),
                 'refused': sum(1 for x in spots if x[1] == 'refused'), 'spots': spots}
            segs.append(s)
            c = chapters.setdefault(s['chapter'], {'segments': 0, 'frames': 0, 'own': 0, 'verified': 0, 'diverged': 0,
                                                   'uncovered': 0, 'tiles': 0, 'exact_tiles': 0, 'refused': 0})
            for key in ('frames', 'verified', 'diverged', 'uncovered', 'tiles', 'exact_tiles', 'refused'):
                c[key] += s[key]
            c['own'] += min(r['matched'], r['frames'])
            c['segments'] += 1
    tot = {k: sum(c[k] for c in chapters.values()) for k in ('segments', 'frames', 'own', 'verified', 'diverged', 'uncovered', 'tiles', 'exact_tiles', 'refused')}
    head = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT).stdout.strip()
    pc = lambda a, b: f'{100.0 * a / b:.1f}' if b else '-'
    L = ['# Parity scoreboard, every-N-frames cuts', '',
         f'Tree: `{head}`. Tiles of {n:,} frames. Regenerate: `scoreboard.py` (the area-entry board, which lists the',
         f'segments), `scoreboard.py mid cut {n}` (local, under `work/real/mid/`), `scoreboard.py mid`.', '',
         f'Every area-entry segment that loses more than {n:,} frames on the main board, cut again at its frame {n:,},',
         f'{2 * n:,}, ... (the any-frame import, `engine/docs/segments.md` §"Scoreboard"). A frame is **verified** when',
         'the segment or a tile replays it exactly from the last cut before it; **diverged** when it comes after a',
         'tile\'s first divergence; **not covered** when its tile was refused by the importer.', '',
         '| chapter | segments | frames | exact on the main board | verified exact | diverged | not covered | tiles | exact tiles | refused |',
         '|---|---|---|---|---|---|---|---|---|---|']
    order = [c for c in ('opening', 'earth', 'ship', 'fat', 'janemba', 'superbuu') if c in chapters]
    for k in order + ['**all**']:
        c = tot if k == '**all**' else chapters[k]
        L.append(f"| {k} | {c['segments']} | {c['frames']:,} | {c['own']:,} ({pc(c['own'], c['frames'])}%) | "
                 f"{c['verified']:,} ({pc(c['verified'], c['frames'])}%) | {c['diverged']:,} | {c['uncovered']:,} | "
                 f"{c['tiles']} | {c['exact_tiles']} | {c['refused']} |")
    # the whole game: every run frame - verified exact (an exact segment, the matched prefix of a segment not tiled,
    # or a tile's), diverged, or not covered (before the run's first importable entry, or in a refused tile)
    tiled = {s['segment']: s for s in segs}
    W = {}
    for d, c in cuts.items():
        w = W.setdefault(c['chapter'], {'frames': 0, 'verified': 0, 'diverged': 0, 'uncovered': 0, 'cut': 0})
        w['frames'] += c['frames']
        for r in res.get(d, []):
            t = tiled.get(r['segment'])
            w['cut'] += r['frames']
            if t:
                w['verified'] += t['verified']
                w['diverged'] += t['diverged']
                w['uncovered'] += t['uncovered']
            else:
                w['verified'] += min(r['matched'], r['frames'])
                w['diverged'] += r['frames'] - min(r['matched'], r['frames'])
    for w in W.values():
        # (frames in no segment: the run's start up to its first importable entry; a chapter's are counted where
        # its segments start, as on the main board)
        w['uncovered'] += max(0, w['frames'] - w['cut'])
    # the opening before its first importable entry (title, New Game, the prologue): the story playthrough test
    # (wf-replay tests/playthrough.rs, no import: the app from power-on) replays the same keys and matches every row
    # of data/story_playthrough/opening.json - credited when the real run's opening keys are that recording's
    pc0 = playthrough_credit()
    if pc0 and 'opening' in W:
        c = min(pc0, W['opening']['uncovered'])
        W['opening']['uncovered'] -= c
        W['opening']['verified'] += c
    WT = {k: sum(w[k] for w in W.values()) for k in ('frames', 'verified', 'diverged', 'uncovered')}
    tot['whole_game'] = WT
    L += ['', '## The whole game', '',
          'Every frame of the real run: verified exact (an exact segment, a segment\'s matched prefix, or a tile), diverged',
          '(after a first divergence, before the next cut), not covered (no importable cut before it: the run\'s start, a',
          'refused tile). The opening\'s title / New Game / prologue before its first area entry count as verified by the',
          f'story playthrough test (same keys from power-on, every row compared, no import: {pc0:,} frames).', '',
          '| chapter | run frames | verified exact | diverged | not covered |', '|---|---|---|---|---|']
    for k in [c for c in ('opening', 'earth', 'ship', 'fat', 'janemba', 'superbuu') if c in W] + ['**all**']:
        w = WT if k == '**all**' else W[k]
        L.append(f"| {k} | {w['frames']:,} | {w['verified']:,} ({pc(w['verified'], w['frames'])}%) | {w['diverged']:,} | "
                 f"{w['uncovered']:,} ({pc(w['uncovered'], w['frames'])}%) |")
    # refusals: what the importer would gain (frames not covered), by its reason
    why = {}
    for s in segs:
        for k, cat, t, *_ in s['spots']:
            if cat == 'refused':
                r = re.sub(r'\d+', 'N', t.split('import: ', 1)[-1])[:110]
                w = why.setdefault(r, [0, 0, set()])
                w[0] += 1
                w[1] += n
                w[2].add(s['chapter'])
    if why:
        L += ['', '## Refused tiles by reason (for the importer)', '', '| reason | tiles | frames | chapters |', '|---|---|---|---|']
        for r, (c, f, chs) in sorted(why.items(), key=lambda x: -x[1][1]):
            L.append(f'| {r} | {c} | {f:,} | {", ".join(sorted(chs))} |')
    L += ['', '## Per segment: where it really diverges', '',
          'segment: main-board matched / frames (its first divergence) -> verified / diverged / not covered; then each',
          'tile divergence (segment frame: category) - a tile that diverges is a separate cause from the one before it', '']
    for ch in order:
        L += [f'### {ch}', '']
        for s in sorted((s for s in segs if s['chapter'] == ch), key=lambda s: -(s['frames'] - s['verified'])):
            L.append(f"* `{s['segment']}` ({s['stretch']}): {s['own']:,} / {s['frames']:,} ({s['own_first'][:90]}) -> "
                     f"**{s['verified']:,}** / {s['diverged']:,} / {s['uncovered']:,}; {s['exact_tiles']} / {s['tiles']} tiles exact")
            for k, cat, t, *_ in s['spots'][:12]:
                L.append(f"  * {k:,}: {cat} - {t[:130]}")
            if len(s['spots']) > 12:
                L.append(f"  * ... {len(s['spots']) - 12} more")
        L.append('')
    (ROOT / 'engine' / 'docs' / 'scoreboard-mid.md').write_text('\n'.join(L))
    (DATA / 'scoreboard_mid.json').write_text(json.dumps({'tree': head, 'tile': n, 'total': tot, 'chapters': chapters,
                                                          'segments': segs}, indent=1, sort_keys=True))
    print('all:', tot)


if __name__ == '__main__':
    a = sys.argv[1:]
    if a[:2] == ['mid', 'cut']:
        do_mid_cut(int(a[2]) if len(a) > 2 else 1000, int(a[3]) if len(a) > 3 else 8, a[4:])
    elif a and a[0] == 'mid':
        mid_board(int(a[1]) if len(a) > 1 else 6)
    elif a and a[0] == 'cut':
        do_cut(int(a[1]) if len(a) > 1 else 6, a[2:])
        do_extend(int(a[1]) if len(a) > 1 else 6)
    elif a and a[0] == 'extend':
        do_extend(int(a[1]) if len(a) > 1 else 6)
    else:
        board(int(a[0]) if a else 6)
