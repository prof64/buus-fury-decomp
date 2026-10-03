"""envt.py STATE [N]: the enemies of a state with their vtable and the first N self-relative vtable entries."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import enemies  # noqa: E402
from route import start_emu  # noqa: E402
sys.path.insert(0, str(ROOT / 're' / 'buu'))
from vtab import entries  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
n = int(sys.argv[2]) if len(sys.argv) > 2 else 16
if '--run' in sys.argv:
    e.run(int(sys.argv[sys.argv.index('--run') + 1]))
for o in enemies(e):
    vt = e.r32(o[0])
    print('enemy type', o[5], 'obj %08X' % o[0], 'vt %08X' % vt, 'hp', o[4])
    for i, f in enumerate(entries(vt, n)):
        print(f'   [{i:2d}] +{4 * i:#04x} -> {f:08x}')
