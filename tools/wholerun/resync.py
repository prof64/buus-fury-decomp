"""resync.py [--lag rec|sim] [--max-stops N] [--from ROW]: the whole run's stop list - the app from the reset with
the run's keys (as board.py); at each stop (the first exact divergence) the game's state is cut at the next row on the
emulator (the importer's any-frame cut: RAM, sound timer phase) and the app goes on from there, until the end of the
recording. Every stop: row, stretch, chapter, what differs, the likely owner. An import the importer refuses, or one
that diverges at once, is an "import" stop and the cut moves on (30, 60, 120 .. rows). Output
work/wholerun/stops_<lag>.jsonl (one line a stop, appended as it goes) - board.py --resync ranks them.
Headless, muted; needs work/wholerun/master.jsonl (record.py) and the wholerun example built (board.py builds it)."""
import gzip
import json
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/story_playthrough', 'tools/field', 'tools/segments', 'tools/script_lockstep', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
import realrun  # noqa: E402
import segment as sg  # noqa: E402
from route import start_emu  # noqa: E402


def phase(state):
    """cycles from the state to the next sound-DMA interrupt (tools/segments/snd_phase.py's measure)"""
    e = start_emu(str(state))
    c0, p0 = e.cycles(), e.r32(0x0300389C)
    for _ in range(200000):
        e.step()
        if e.r32(0x0300389C) != p0:
            return e.cycles() - c0
    return None

a = sys.argv[1:]
lag = a[a.index('--lag') + 1] if '--lag' in a else 'rec'
max_stops = int(a[a.index('--max-stops') + 1]) if '--max-stops' in a else 5000
start_row = int(a[a.index('--from') + 1]) if '--from' in a else 0
W = ROOT / 'work' / 'wholerun'
CUTS = W / 'cuts'
CUTS.mkdir(parents=True, exist_ok=True)
master = W / 'master.jsonl'
idx = json.loads((W / 'master.jsonl.index.json').read_text())
n_rows = idx['rows']
subprocess.run(['cargo', 'build', '--release', '--target-dir', str(ROOT / 'engine' / 'target-wholerun'), '-p', 'wf-app', '--example', 'wholerun'],
               cwd=ROOT / 'engine', check=False)
exe = ROOT / 'engine' / 'target-wholerun' / 'release' / 'examples' / 'wholerun'

# the chain's keys (as record.py)
keys = []
for _, name, part in realrun.PRE:
    keys += realrun.pre_keys()[part]
for m in json.loads((ROOT / 'data' / 'realrun' / 'manifest.json').read_text()):
    keys += realrun.unrle(json.loads(gzip.open(ROOT / 'data' / 'realrun' / f'{m["n"]:03d}_{m["name"]}.keys.gz', 'rt').read()))
keys = keys[:n_rows]

man = {f'{m["n"]:03d}_{m["name"]}': m['chapter'] for m in json.loads((ROOT / 'data' / 'realrun' / 'manifest.json').read_text())}
OWNER = {'opening': 'Earth agent', 'earth': 'Earth agent', 'ship': 'ship agent', 'fat': 'dungeon agents A / B',
         'janemba': 'side-content agent', 'superbuu': 'Super Buu agent'}


def chapter(stretch):
    if stretch.startswith('E1'):
        return 'opening'
    if stretch.startswith('E2'):
        return 'earth'
    return man.get(stretch.split(' ')[0], '?')


def owner(stop):
    if stop['field'] == 'import' or stop.get('kind') == 'import':
        return 'importer agent'
    if stop.get('menu'):
        return 'menus: Earth agent (menu timing) / soak agent'
    return OWNER.get(chapter(stop.get('stretch', '')), '?')


e = start_emu('reset')
cur = 0


def cut_at(r):
    """the game's state before row r: a segment doc the importer reads (RAM + sound phase), 2 recorded rows"""
    global cur, e
    while cur < r:
        e.set_keys(keys[cur])
        e.run(1)
        cur += 1
    st = CUTS / f'c{r}.ss'
    e.save_file(str(st))
    e2 = start_emu(str(st))
    init, objs, rows = sg.record(e2, lambda j, _e: keys[r + j] if r + j < len(keys) else None, 2)
    init['snd_phase'] = phase(st)
    doc = {'kind': 'segment', 'name': f'wr_{r}', 'source': {'kind': 'state', 'state': str(st)}, 'init': init, 'objs': objs, 'frames': rows}
    p = CUTS / f'c{r}.json'
    p.write_text(json.dumps(doc, separators=(',', ':')))
    return p


def run(from_row, seg):
    cmd = [str(exe), str(master), '--lag', lag, '--stop-first']
    if seg is None:
        cmd += ['--power-on', str(W / 'power_on.json')]
    else:
        cmd += ['--import', str(seg), '--from-row', str(from_row)]
    out = subprocess.run(cmd, cwd=ROOT / 'engine', capture_output=True, text=True).stdout
    for line in out.splitlines():
        if line.startswith('STOP '):
            return json.loads(line[5:])
    return None


log = W / f'stops_{lag}.jsonl'
if start_row == 0:
    log.write_text('')
seg, row, stops, t0 = None, start_row, 0, time.time()
if start_row:
    seg = cut_at(start_row)
jump = 30
while stops < max_stops:
    s = run(row, seg)
    if s is None:
        print(f'end of the recording reached from row {row}', flush=True)
        break
    s['from'] = row
    s['chapter'] = chapter(s.get('stretch', ''))
    # an import refused, or a divergence in the cut's first rows: the importer's, not a rule's - move the cut on
    if s['field'] == 'import' or (seg is not None and s['row'] - row < 2):
        s['kind'] = 'import'
        nxt = row + jump
        jump = min(jump * 2, 4000)
    else:
        s['kind'] = 'rule'
        nxt = s['row'] + 1
        jump = 30
    s['owner'] = owner(s)
    with log.open('a') as fh:
        fh.write(json.dumps(s) + '\n')
    stops += 1
    print(f"{stops} {s['kind']} row {s['row']} ({s.get('stretch', '')}) {s['field']}: {str(s.get('what'))[:90]} -> cut {nxt} ({time.time() - t0:.0f}s)", flush=True)
    if nxt >= n_rows - 3:
        break
    row = nxt
    try:
        seg = cut_at(row)
    except Exception as ex:  # noqa: BLE001 - a cut the recorder cannot make: skip on
        print('cut failed', row, ex, flush=True)
        row += jump
        seg = cut_at(row)
print('stops', stops)
