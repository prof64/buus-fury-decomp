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
CATS = [
    ('importer', ('import', 'not importable', 'unsupported')),
    ('sound', ('sfx', 'sound', 'song', 'music', 'snd')),
    ('menu timing', ('menu', 'redraw', 'status page', 'lag')),
    ('sprite memory', ('tile', 'vram', 'oam', 'slot', 'alloc')),
    ('RNG', ('rng', 'rand')),
    ('script', ('script', 'flag', 'event', 'scene', 'depth', 'dialog', 'text')),
    ('animation', ('anim', 'cel', 'pose', 'raw ')),
    ('position', ('pos', ' x ', ' y ', 'player', 'camera', 'facing', 'hp', 'actor', 'enemy', 'obj')),
]
OWNER = {'menu timing': 'Earth agent (redraw / mixer timing predictor)', 'sound': 'Earth agent (sound timing) / audio',
         'importer': 'importer (wf-import)', 'sprite memory': 'chapter agent (tile allocator order)',
         'RNG': 'chapter agent', 'script': 'cutscene / script-host agent', 'animation': 'chapter agent',
         'position': 'chapter agent', 'other': '-'}
CHAPTER_OWNER = {'ship': 'ship agent', 'fat': 'dungeon agents A / B (+ ship agent: Thieves Den, Pilaf)',
                 'janemba': 'side-content agent', 'superbuu': 'Super Buu agent'}


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


def do_cut(jobs, only):
    subprocess.run([PY, str(ROOT / 'tools/segments/realrun.py'), 'runs', str(REAL / 'runs')], check=True, cwd=ROOT)
    man = {f'{m["n"]:03d}_{m["name"]}': m for m in json.loads((DATA / 'manifest.json').read_text())}
    dirs = [d for d in sorted((REAL / 'runs').iterdir()) if d.is_dir() and (not only or d.name[:3] in only)]
    p = REAL / 'cuts.json'
    cuts = json.loads(p.read_text()) if p.exists() else {}
    with ThreadPoolExecutor(jobs) as ex:
        for name, n, cut, tail in ex.map(cut_one, dirs):
            cuts[name] = {'chapter': man[name]['chapter'], 'frames': man[name]['frames'], 'cut_frames': cut, 'segments': n}
            print(name, n, 'segments', cut, '/', man[name]['frames'], 'frames', *tail, flush=True)
            p.write_text(json.dumps(cuts, indent=0, sort_keys=True))


def replay_one(d):
    env = dict(os.environ, SEGMENT_DIR=str(d), SEGMENT_STRICT='0')
    r = subprocess.run(CARGO, capture_output=True,
                       text=True, cwd=ROOT / 'engine', env=env)
    rows = []
    for ln in r.stderr.splitlines():
        m = LINE.match(ln)
        if m:
            rows.append({'segment': m.group(1), 'frames': int(m.group(2)), 'matched': int(m.group(4)),
                         'first': m.group(6)})
    return d.name, rows, r.returncode


def board(jobs):
    subprocess.run(CARGO + ['--no-run'], check=True,
                   cwd=ROOT / 'engine', capture_output=True)
    cuts = json.loads((REAL / 'cuts.json').read_text())
    dirs = [d for d in sorted((REAL / 'segs').iterdir()) if d.is_dir() and any(d.glob('*.json'))]
    res = {}
    with ThreadPoolExecutor(jobs) as ex:
        for name, rows, rc in ex.map(replay_one, dirs):
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
         'stay in the segment before it. "not cut" = the frames before a stretch\'s first importable entry.', '',
         '| chapter | owner | run frames | not cut | segments | exact | frames | matched | % |', '|---|---|---|---|---|---|---|---|---|']
    order = [c for c in ('ship', 'fat', 'janemba', 'superbuu') if c in chapters]
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
    L += ['', '## Every non-exact segment', '', 'segment (stretch): matched / frames, category - first divergence', '']
    for ch in order:
        L.append(f'### {ch}')
        L.append('')
        for c, name, r, cat in bad:
            if c == ch:
                L.append(f"* `{r['segment']}` ({name}): {r['matched']:,} / {r['frames']:,}, **{cat}** - {r['first'][:160]}")
        L.append('')
    (ROOT / 'engine' / 'docs' / 'scoreboard.md').write_text('\n'.join(L))
    (DATA / 'scoreboard.json').write_text(json.dumps({'tree': head, 'dirty': dirty, 'total': tot, 'chapters': chapters}, indent=1, sort_keys=True))
    print('all:', tot)


if __name__ == '__main__':
    a = sys.argv[1:]
    if a and a[0] == 'cut':
        do_cut(int(a[1]) if len(a) > 1 else 6, a[2:])
    else:
        board(int(a[0]) if a else 6)
