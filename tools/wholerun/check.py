"""check.py [--lag none|rec] [--known FILE]: "is it still exact?" - the whole real run (power-on to the credits, 1.67M
rows) through the app in no-lag mode, compared per logic iteration with the emulator's recording: RNG, area, position,
script depth, story flags and the sound requests. Exit 0 = exact everywhere outside the known ranges
(data/realrun/known_stops.json); 1 = a new stop (printed: row, stretch, what); 2 = setup missing.

Legs, run in parallel: the app from power-on (no import) up to the first known range's first row, then from the game's
state cut right after each known range up to the next one / the end. A cut is made once (work/wholerun/cuts/c<ROW>.json
via resync.py, ~10 min of emulator replay) and reused. Headless, muted. ~20 s once built.
Needs work/wholerun/master.jsonl + power_on.json (tools/wholerun/board.py makes both)."""
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
W = ROOT / 'work' / 'wholerun'
a = sys.argv[1:]
lag = a[a.index('--lag') + 1] if '--lag' in a else 'none'
master, power_on = W / 'master.jsonl', W / 'power_on.json'
sounds = ROOT / 'data' / 'realrun' / 'sounds.json.gz'
for p in (master, power_on):
    if not p.exists():
        print(f'missing {p} - run tools/wholerun/board.py first')
        sys.exit(2)
rows = json.loads((W / 'master.jsonl.index.json').read_text())['rows']
known_file = Path(a[a.index('--known') + 1]) if '--known' in a else ROOT / 'data' / 'realrun' / 'known_stops.json'
known = sorted(json.loads(known_file.read_text())['stops'], key=lambda k: k['from'])

tgt = ROOT / 'engine' / 'target-wholerun'
b = subprocess.run(['cargo', 'build', '--release', '--target-dir', str(tgt), '-p', 'wf-app', '--example', 'wholerun'],
                   cwd=ROOT / 'engine', capture_output=True, text=True)
if b.returncode:
    print(b.stderr[-3000:])
    sys.exit(2)
exe = tgt / 'release' / 'examples' / 'wholerun'

# legs: (start row, end row exclusive, cut doc or None = power-on)
legs, start = [], 0
for k in known:
    legs.append((start, k['from']))
    start = k['to'] + 1
legs.append((start, rows))
for s, _ in legs:
    if s and not (W / 'cuts' / f'c{s}.json').exists():
        print(f'making the cut at row {s} (once, ~10 min)', flush=True)
        subprocess.run([sys.executable, str(ROOT / 'tools' / 'wholerun' / 'resync.py'), '--lag', lag, '--from', str(s),
                        '--max-stops', '1', '--log', str(W / 'cuts' / 'check_cut.jsonl')], cwd=ROOT, capture_output=True)
        if not (W / 'cuts' / f'c{s}.json').exists():
            print(f'could not make the cut at row {s}')
            sys.exit(2)


def run(leg):
    s, e = leg
    rep = W / f'check_{lag}_{s}.json'
    cmd = [str(exe), str(master), '--lag', lag, '--frames', str(e), '--report', str(rep)]
    if lag == 'none' and sounds.exists():
        cmd += ['--sounds', str(sounds)]
    cmd += ['--import', str(W / 'cuts' / f'c{s}.json'), '--from-row', str(s)] if s else ['--power-on', str(power_on)]
    p = subprocess.run(cmd, cwd=ROOT / 'engine', capture_output=True, text=True)
    if p.returncode or not rep.exists():
        return leg, None, (p.stdout + p.stderr)[-2000:]
    return leg, json.loads(rep.read_text()), ''


bad = 0
with ThreadPoolExecutor(len(legs)) as ex:
    for (s, e), r, err in ex.map(run, legs):
        name = 'power-on' if s == 0 else f'cut {s:,}'
        if r is None:
            print(f'FAIL {name} .. {e:,}: the app did not finish\n{err}')
            bad += 1
            continue
        probs = []
        if r['exact_to'] < e:
            probs.append(f"stop at row {r['exact_to']:,} ({r['exact_stretch']}): {r['first']}")
        sf = r.get('sounds_first')
        if sf:
            probs.append(f"sound at row {sf['row']:,} ({sf['stretch']}): {sf['what'][:160]} ({r['sounds_bad']} iterations)")
        if r['script_errors']:
            probs.append(f"{r['script_errors']} script errors")
        if probs:
            bad += 1
            print(f'FAIL {name} .. {e:,}: ' + ' | '.join(probs))
        else:
            snd = f", sounds {r['sounds_iters']:,} iterations" if r.get('sounds_compared') else ''
            print(f'ok   {name} .. {e:,}: exact ({r["stepped"]:,} logic iterations{snd})')
skipped = ', '.join(f"{k['from']:,}..{k['to']:,} ({k['stretch']})" for k in known)
print(('NEW STOP(S)' if bad else 'EXACT') + f' - whole run, {lag} lag, {rows:,} rows; known ranges skipped: {skipped or "none"}')
sys.exit(1 if bad else 0)
