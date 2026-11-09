"""runs.py TARGET_DIR: the whole-run replays (tools/wholerun/check.py's legs: every real run through the app in no-lag
mode, compared per logic iteration with the emulator's recording) with the coverage recorder on, level `run`. Its
counts (`n`: logic iterations a thing was present in) are the report's "how often a player meets it". Reports go to
work/coverage/runs/ (check.py's own files in work/wholerun/ are not touched). Headless, muted."""
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
import chains  # noqa: E402

tgt = ROOT / 'engine' / (sys.argv[1] if len(sys.argv) > 1 else 'target')
raw = Path(os.environ.get('COVERAGE_RAW', ROOT / 'work' / 'coverage' / 'raw'))
out = ROOT / 'work' / 'coverage' / 'runs'
out.mkdir(parents=True, exist_ok=True)
for f in raw.glob('run-*.jsonl'):
    f.unlink()
b = subprocess.run(['cargo', 'build', '--release', '--target-dir', str(tgt), '-p', 'wf-app', '--example', 'wholerun'],
                   cwd=ROOT / 'engine', capture_output=True, text=True)
if b.returncode:
    print(b.stderr[-3000:])
    sys.exit(2)
exe = tgt / 'release' / 'examples' / 'wholerun'
legs = []
for chain in chains.NAMES:
    P = chains.paths(chain)
    if not P['master'].exists():
        print(f'{chain}: no master recording - skipped')
        continue
    rows = json.loads((P['master'].parent / (P['master'].name + '.index.json')).read_text())['rows']
    known = sorted(json.loads(P['known'].read_text())['stops'], key=lambda k: k['from']) if P['known'].exists() else []
    start = 0
    for k in known:
        legs.append((chain, start, k['from']))
        start = k['to'] + 1
    legs.append((chain, start, rows))


def run(leg):
    chain, s, e = leg
    P = chains.paths(chain)
    rep = out / f'{chain}_{s}.json'
    cmd = [str(exe), str(P['master']), '--lag', 'none', '--frames', str(e), '--report', str(rep)]
    if s:
        cut = P['cuts'] / f'c{s}.json'
        if not cut.exists():
            return leg, f'no cut {cut}'
        cmd += ['--import', str(cut), '--from-row', str(s)]
    else:
        cmd += ['--power-on', str(chains.W / 'power_on.json')]
    env = dict(os.environ, WF_COVERAGE=str(raw), WF_COVERAGE_LEVEL='run', WF_COVERAGE_CTX=f'{chain}@{s}')
    p = subprocess.run(cmd, cwd=ROOT / 'engine', capture_output=True, text=True, env=env)
    if p.returncode or not rep.exists():
        return leg, (p.stdout + p.stderr)[-600:]
    r = json.loads(rep.read_text())
    return leg, f"exact to row {r['exact_to']:,} of {e:,}, {r['stepped']:,} iterations"


with ThreadPoolExecutor(max(1, len(legs))) as ex:
    for leg, msg in ex.map(run, legs):
        print(f'run: {leg[0]} rows {leg[1]:,}..{leg[2]:,}: {msg}')
