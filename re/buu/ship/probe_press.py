"""probe_press.py STATE.ss "[(x, y, dir), ...]" FLAG -- for each spot: walk there, face, press A, report whether FLAG got set
(or an event / script started)"""
import ast
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ship_driver import ShipDriver, status  # noqa: E402
from route import start_emu  # noqa: E402

GD = 0x03001068
st = os.path.abspath(sys.argv[1])
flag = int(sys.argv[3])
for x, y, d in ast.literal_eval(sys.argv[2]):
    e = start_emu(st)
    dr = ShipDriver([('walk', x, y), ('face', d), ('press', 1), ('wait', 500)])
    sc = 0
    while not dr.done():
        e.set_keys(dr.keys(e))
        e.run(1)
        sc = max(sc, status(e)['scene'])
    f = e.r8(GD + 0x55 + (flag >> 3)) >> (flag & 7) & 1
    print((x, y, d), 'at', status(e)['p'], 'flag', f, 'scene', sc)
