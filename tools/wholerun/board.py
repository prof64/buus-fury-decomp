"""board.py [--record] [--frames N]: the end-to-end test - the whole real run fed to the app from the reset with no RAM
import anywhere, compared row by row with the emulator's recording of the same keys - and its board line.

  1. `record.py` (only with --record or when work/wholerun/master.jsonl is missing): the emulator from the soft reset
     (`route.start_emu('reset')`, the board's power-on), the opening + Earth keys, then data/realrun/*.keys.gz in order;
     one row per video frame (~11 min).
  2. `power_on.py`: the sound hardware at the reset (driver RAM as the boot code sets it + the reset's timer phase and
     play position - the app's power-on sound clock; nothing else of the RAM).
  3. The app (`engine/crates/wf-app/examples/wholerun.rs`, headless, muted, target dir engine/target-wholerun) in both
     lag modes: `rec` = the recording's lag frames injected, `sim` = the sim's own load / text-box lag
     (`Options::sim_lag`, as SEGMENT_SIMLAG).
  4. data/realrun/wholerun.json (read by scoreboard.py for its line "Whole run, no imports: exact to frame N").
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
W = ROOT / 'work' / 'wholerun'
PY = sys.executable
a = sys.argv[1:]
master = W / 'master.jsonl'
if '--record' in a or not master.exists():
    subprocess.run([PY, str(ROOT / 'tools' / 'wholerun' / 'record.py'), str(master)], check=False)
if not (W / 'power_on.json').exists():
    subprocess.run([PY, str(ROOT / 'tools' / 'wholerun' / 'power_on.py')], check=True)
tgt = ROOT / 'engine' / 'target-wholerun'
subprocess.run(['cargo', 'build', '--release', '--target-dir', str(tgt), '-p', 'wf-app', '--example', 'wholerun'], cwd=ROOT / 'engine', check=True)
exe = tgt / 'release' / 'examples' / 'wholerun'
modes = []
for lag in ('rec', 'sim'):
    rep = W / f'report_{lag}.json'
    cmd = [str(exe), str(master), '--power-on', str(W / 'power_on.json'), '--lag', lag, '--report', str(rep)]
    if '--frames' in a:
        cmd += ['--frames', a[a.index('--frames') + 1]]
    log = W / f'run_{lag}.log'
    with log.open('w') as fh:
        subprocess.run(cmd, cwd=ROOT / 'engine', stdout=fh, stderr=subprocess.STDOUT, check=True)
    r = json.loads(rep.read_text())
    modes.append({k: r[k] for k in ('lag', 'rows', 'exact_to', 'exact_stretch', 'first', 'lasting', 'sim_lag', 'sim_lag_on_logic', 'recorded_lag_replayed', 'script_errors')})
    print(f"{r['lag']}: exact to row {r['exact_to']:,} of {r['rows']:,} ({r['exact_stretch']}) - {r['first']}")
idx = json.loads((W / 'master.jsonl.index.json').read_text()) if (W / 'master.jsonl.index.json').exists() else {}
note = ''
if any('diverges' in s[0] for s in idx.get('stretches', [])):
    note = ('The recording follows the real run from the reset through 002_d04_bandana only: replayed from the reset, '
            'the key log of 003_d06_thieves leaves the real run (the chain\'s state at 002\'s end differs from the '
            'local state 003 was packed from - object list, heap, one video frame) and ends in a game over.')
head = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT).stdout.strip()
(ROOT / 'data' / 'realrun' / 'wholerun.json').write_text(json.dumps({'tree': head, 'modes': modes, 'note': note}, indent=1))
print('wrote data/realrun/wholerun.json')
