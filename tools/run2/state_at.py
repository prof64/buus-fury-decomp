"""state_at.py ROW OUT.ss [--chain run2]: the emulator's state before chain row ROW, saved to OUT.ss - replayed from
the chain's nearest cut state (<chain>/cuts/c<N>.ss, N <= ROW) with the recorded keys (headless, muted). For making
menu_parity / route-planning states somewhere a real run passes (e.g. the Capsule Corp lobby, run 2 row 74,943)."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'wholerun'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
import chains  # noqa: E402
from route import start_emu, status  # noqa: E402

a = sys.argv[1:]
row, out = int(a[0]), a[1]
chain = chains.chain_arg(a) if '--chain' in a else 'run2'
P = chains.paths(chain)
cuts = sorted(int(p.stem[1:]) for p in P['cuts'].glob('c*.ss') if p.stem[1:].isdigit() and int(p.stem[1:]) <= row)
keys = []
for _, ks, _ in chains.parts(chain):
    keys += ks
start = cuts[-1] if cuts else 0
if not start:
    sys.exit('no cut state at or before the row (power-on replay not supported here)')
e = start_emu(str(P['cuts'] / f'c{start}.ss'))
for r in range(start, row):
    e.set_keys(keys[r])
    e.run(1)
e.set_keys(0)
e.save_file(out)
print(f'{chain} row {row} (from cut {start}) -> {out}', status(e))
