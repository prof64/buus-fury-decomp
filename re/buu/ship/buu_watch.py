"""buu_watch.py STATE.ss -- the 8-71 fight with the plain fight goal (Babidi not excluded): every write to Buu's
pacified word 0x0300302C (value, pc, lr) and Buu's HP every 500 frames"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ship_driver  # noqa: E402
from ship_driver import ShipDriver, status, enemies  # noqa: E402
from route import start_emu  # noqa: E402

ship_driver.ShipDriver.AVOID = (20, 64)
ship_driver.ShipDriver.BUU = ()
e = start_emu(os.path.abspath(sys.argv[1]))
d = ShipDriver([('story', 8, 70, True)])
e.watch(0x0300302C, 0x03003030, 'w')
for f in range(int(sys.argv[2]) if len(sys.argv) > 2 else 9000):
    e.set_keys(d.keys(e))
    e.run(1)
    for h in e.hits():
        r = h.get('regs') or []
        print(f, {k: (hex(v) if isinstance(v, int) else v) for k, v in h.items() if k != 'regs'},
              'val', e.r32(0x0300302C), 'lr', hex(r[14]) if len(r) > 14 else None, flush=True)
    if f % 500 == 0 or (len(sys.argv) > 3 and int(sys.argv[3]) <= f <= int(sys.argv[3]) + 40):
        b = next((o for o in enemies(e) if o[5] == 27), None)
        bb = next((o for o in enemies(e) if o[5] == 29), None)
        print(f, "buu", b and b[4] >> 10, b and b[1:3], "hp", status(e)["hp"], "p", status(e)["p"], "babidi", bb and bb[1:4], bb and [hex(v) for v in ship_driver.ai_state(e, bb[0])], flush=True)
