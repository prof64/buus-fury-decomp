"""poke.py STATE PROGRAM [--shot name] [--save out.ss]: load a state, play a key program ('R:20,U:30,A:3,0:20'),
print the status after each step (debugging routes)."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
from ship_driver import status, enemies  # noqa: E402
from route import start_emu, keys_of  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
print('start', status(e))
for seg in sys.argv[2].split(','):
    k, n = seg.split(':')
    e.set_keys(keys_of(k))
    e.run(int(n))
    print(seg, status(e), len(enemies(e)))
e.set_keys(0)
if '--shot' in sys.argv:
    e.screenshot('/tmp/poke_%s.png' % sys.argv[sys.argv.index('--shot') + 1], 2)
if '--save' in sys.argv:
    e.save_file(sys.argv[sys.argv.index('--save') + 1])
