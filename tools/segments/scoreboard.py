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


if __name__ == '__main__':
    a = sys.argv[1:]
    if a and a[0] == 'cut':
        do_cut(int(a[1]) if len(a) > 1 else 6, a[2:])
        do_extend(int(a[1]) if len(a) > 1 else 6)
    elif a and a[0] == 'extend':
        do_extend(int(a[1]) if len(a) > 1 else 6)
    else:
        board(int(a[0]) if a else 6)
