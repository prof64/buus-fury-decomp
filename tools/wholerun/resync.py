"""resync.py [--lag rec|sim|none] [--max-stops N] [--from ROW] [--log FILE]: the whole run's stop list - the app from the reset with
the run's keys (as board.py); at each stop (the first exact divergence) the game's state is cut at the next row on the
emulator (the importer's any-frame cut: RAM, sound timer phase) and the app goes on from there, until the end of the
recording. Every stop: row, stretch, chapter, what differs, the likely owner. An import the importer refuses, or one
that diverges at once, is an "import" stop and the cut moves on (30, 60, 120 .. rows). Output
work/wholerun/stops_<lag>.jsonl (one line a stop, appended as it goes) - board.py --resync ranks them.
Headless, muted; needs work/wholerun/master.jsonl (record.py) and the wholerun example built (board.py builds it)."""
import base64
import gzip
import json
import subprocess
import sys
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 'tools/segments', 'tools/script_lockstep', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
from sbemu import sd_read  # noqa: E402  (the savedata-capable emulator lib first: the cuts carry the EEPROM)
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
sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
import chains  # noqa: E402
# --chain run1|run2 (chains.py): which real run (default run1)
chain = chains.chain_arg(a)
P = chains.paths(chain)
W = P['dir']
CUTS = P['cuts']
CUTS.mkdir(parents=True, exist_ok=True)
master = P['master']
idx = json.loads((master.parent / (master.name + '.index.json')).read_text())
n_rows = idx['rows']
subprocess.run(['cargo', 'build', '--release', '--target-dir', str(ROOT / 'engine' / 'target-wholerun'), '-p', 'wf-app', '--example', 'wholerun'],
               cwd=ROOT / 'engine', check=False)
exe = ROOT / 'engine' / 'target-wholerun' / 'release' / 'examples' / 'wholerun'

# the chain's keys (as record.py)
keys = []
for _, ks, _ in chains.parts(chain):
    keys += ks
keys = keys[:n_rows]

man = {f'{m["n"]:03d}_{m["name"]}': m['chapter'] for m in json.loads(P['manifest'].read_text())}
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


CUT_ROWS = 64
CUT_ITERATIONS = 2


def trim_to_iteration(rows, n=CUT_ITERATIONS):
    """the rows through the end of the cut's n-th game iteration (the last row before the logic counter moves the n-th
    time): the import's check, as the 2-row docs were, but never ending inside a long iteration - the segment harness
    compares a doc's last row as an iteration's end, and a 2-row doc that stopped inside a load / a text box still
    being made read a miss there the app does not have (whole-run cuts 568,177 / 633,276)"""
    ends = [k for k in range(len(rows) - 1) if rows[k + 1]['lf'] != rows[k]['lf']]
    return rows[:ends[n - 1] + 1] if len(ends) >= n else rows


def cut_at(r):
    """the game's state before row r: a segment doc the importer reads (RAM + sound phase) and up to CUT_ROWS recorded
    rows, ending at an iteration's end (`trim_to_iteration`)"""
    global cur, e
    while cur < r:
        e.set_keys(keys[cur])
        e.run(1)
        cur += 1
    st = CUTS / f'c{r}.ss'
    e.save_file(str(st))
    e2 = start_emu(str(st))
    init, objs, rows = sg.record(e2, lambda j, _e: keys[r + j] if r + j < len(keys) else None, CUT_ROWS)
    rows = trim_to_iteration(rows)
    init['snd_phase'] = phase(st)
    # the cartridge's EEPROM (the in-game saves): a save state does not carry it, the continuous session does - the
    # importer fills the app's save slots from it (a Continue after a game over, run 2 row 35,806)
    init['eeprom'] = base64.b64encode(zlib.compress(sd_read(e))).decode()
    doc = {'kind': 'segment', 'name': f'wr_{r}', 'source': {'kind': 'state', 'state': str(st)}, 'init': init, 'objs': objs, 'frames': rows}
    p = CUTS / f'c{r}.json'
    p.write_text(json.dumps(doc, separators=(',', ':')))
    return p


def run(from_row, seg):
    cmd = [str(exe), str(master), '--lag', lag, '--stop-first']
    if seg is None:
        cmd += ['--power-on', str(chains.W / 'power_on.json')]
    else:
        cmd += ['--import', str(seg), '--from-row', str(from_row)]
    out = subprocess.run(cmd, cwd=ROOT / 'engine', capture_output=True, text=True).stdout
    for line in out.splitlines():
        if line.startswith('STOP '):
            return json.loads(line[5:])
    return None


# known differences by design (each documented in engine/docs/segments.md): a rule stop inside one is not chased -
# the run is cut again after the range's end
KNOWN = json.loads(P['known'].read_text())['stops'] if P['known'].exists() else []
log = Path(a[a.index('--log') + 1]) if '--log' in a else W / f'stops_{lag}.jsonl'
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
    known = next((k for k in KNOWN if k['from'] <= s['row'] <= k['to']), None)
    if known is not None and s['field'] != 'import':
        # a known difference by design (data/realrun/known_stops.json): logged, and the run is cut again after it
        s['kind'] = 'known'
        s['known'] = known['what'][:120]
        nxt = known['to'] + 1
        jump = 30
    elif s['field'] == 'import' or (seg is not None and s['row'] - row < 2):
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
