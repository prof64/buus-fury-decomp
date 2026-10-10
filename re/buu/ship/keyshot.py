"""keyshot.py STATE.ss "KEYS" OUTPREFIX -- run a key program [(mask, on, off), ...] from a state, saving a screenshot
OUTPREFIX_<i>.png after every step (menu exploration) and the final state OUTPREFIX.ss"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import KEY, status  # noqa: E402
from route import start_emu  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))
prog = eval(sys.argv[2], {'KEY': KEY})
out = os.path.abspath(sys.argv[3])
for i, (m, on, off) in enumerate(prog):
    e.set_keys(m)
    e.run(on)
    e.set_keys(0)
    e.run(off)
    e.screenshot(f'{out}_{i}.png', 1)
    print(i, hex(m), status(e))
e.save_file(out + '.ss')
