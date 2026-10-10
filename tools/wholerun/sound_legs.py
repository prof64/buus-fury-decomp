"""sound_legs.py [--chain run1|run2] [--jobs N] [--out FILE]: the game's sound requests vs the app's, per logic
iteration, along the WHOLE chain - also past logic stops: one leg per existing resync cut (`<chain dir>/cuts/c<ROW>`,
resync.py / check.py make them) up to the next cut, each run with `--stop-first` so only rows the app replays exactly
are compared (a leg's sounds end at its first logic stop). Headless, muted. Output: OUT (default
`<chain dir>/sound_diffs.json`): every differing iteration {row, stretch, chapter, lf, app, game}, and a summary on
stdout - the differences grouped by (sound id, what kind: app only / game only / both differ) with counts, first
rows and the stretches they come in. Needs the chain's master + sounds (record.py, sounds.py) and its cuts."""
import json
import re
import subprocess
import sys
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
import chains  # noqa: E402

a = sys.argv[1:]
chain = chains.chain_arg(a)
P = chains.paths(chain)
jobs = int(a[a.index('--jobs') + 1]) if '--jobs' in a else 6
out = Path(a[a.index('--out') + 1]) if '--out' in a else P['dir'] / 'sound_diffs.json'
idx = json.loads((Path(str(P['master']) + '.index.json')).read_text())
n_rows = idx['rows']
stretches = idx['stretches']
TD = ROOT / 'engine' / 'target-soak'
subprocess.run(['cargo', 'build', '-q', '--release', '--target-dir', str(TD), '-p', 'wf-app', '--example', 'wholerun'],
               cwd=ROOT / 'engine', check=True)
exe = TD / 'release' / 'examples' / 'wholerun'


def stretch_of(row):
    s = None
    for name, r0 in stretches:
        if r0 <= row:
            s = name
    return s or '?'


cuts = sorted(int(p.stem[1:]) for p in P['cuts'].glob('c*.json'))
legs = []
starts = [0] + cuts
for i, s in enumerate(starts):
    e = starts[i + 1] if i + 1 < len(starts) else n_rows
    if e > s:
        legs.append((s, e))
SND = re.compile(r'^SND row (\d+) \(([^)]*)\) lf (-?\d+) menu (\w+): app (.*) game (.*)$')


def run(leg):
    s, e = leg
    cmd = [str(exe), str(P['master']), '--lag', 'none', '--frames', str(e), '--stop-first',
           '--sounds', str(P['sounds']), '--sounds-show', '1000000']
    cmd += ['--import', str(P['cuts'] / f'c{s}.json'), '--from-row', str(s)] if s else \
        ['--power-on', str(chains.W / 'power_on.json')]
    p = subprocess.run(cmd, cwd=ROOT / 'engine', capture_output=True, text=True)
    diffs, stop = [], None
    for line in p.stdout.splitlines():
        m = SND.match(line)
        if m:
            diffs.append({'row': int(m[1]), 'stretch': m[2], 'lf': int(m[3]), 'menu': m[4] == 'true',
                          'app': json.loads(m[5]), 'game': json.loads(m[6])})
        elif line.startswith('STOP '):
            stop = json.loads(line[5:]).get('row')
    return leg, diffs, stop, (p.stderr[-300:] if p.returncode else '')


allv, cover = [], 0
with ThreadPoolExecutor(jobs) as ex:
    for (s, e), diffs, stop, err in ex.map(run, legs):
        end = stop if stop is not None else e
        cover += max(0, end - s)
        # (a leg's first row from a cut: the import's own song request - the app restarts the area's song - is not a
        # game difference. A logic stop reported at row N diverged in iteration N - 1 (the state is compared at the
        # next row): that iteration's sounds belong to the stop - run 3 1,947,900 from the cut 1,947,883 hit two of
        # Janemba's three parts (an import gap), its punch sounds differed and the RNG stop showed at 1,947,901; from
        # power-on the row is exact in state and sound)
        allv += [d for d in diffs if (stop is None or d['row'] < stop - 1) and not (s and d['row'] == s and
                                                                              d['game'] == [] and
                                                                              all(x.startswith('song') for x in d['app']))]
        if err:
            print(f'leg {s}: {err}', flush=True)
allv.sort(key=lambda d: d['row'])
out.write_text(json.dumps({'chain': chain, 'rows_compared': cover, 'diffs': allv}))
print(f'{chain}: {len(legs)} legs, {cover:,} rows replayed exactly, {len(allv)} iterations with a sound difference')


def ids(evs):
    return Counter(evs)


groups = defaultdict(list)
for d in allv:
    app, game = Counter(d['app']), Counter(d['game'])
    for ev in set(app) | set(game):
        if app[ev] != game[ev]:
            kind = 'app only' if app[ev] > game[ev] else 'game only'
            groups[(ev.rsplit(' ', 1)[0] if ev.startswith('play') else ev, kind)].append(d)
for (ev, kind), ds in sorted(groups.items(), key=lambda g: -len(g[1])):
    st = Counter(d['stretch'] for d in ds)
    print(f'{len(ds):6d}  {ev:14s} {kind:9s} first row {ds[0]["row"]:,} ({ds[0]["stretch"]})  stretches {dict(st.most_common(4))}')
