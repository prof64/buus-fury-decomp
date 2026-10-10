"""script_trace_shot.py state [frames] [keys...] -- load a state, run, press a key sequence (each key name
held 4 frames then 20 idle frames), save a screenshot to work/buu/script/shot.png. Helper for scenario setup."""
import sys
from pathlib import Path
from common import *

st = sys.argv[1]
frames = int(sys.argv[2]) if len(sys.argv) > 2 else 2
e = emu(st if Path(st).is_absolute() or (STATES / st).exists() else WORK / st)
e.run(frames)
for k in sys.argv[3:]:
    if k.startswith('w'):
        e.run(int(k[1:]))
        continue
    e.press(getattr(KEY, k), frames=4, after=20)
(WORK / 'script').mkdir(exist_ok=True)
e.screenshot(WORK / 'script' / 'shot.png')
p = e.r32(0x03001c34)
print('area', hex(e.r32(0x03001980)), 'player', hex(p), 'pos', (e.r32(p + 0x1a0) >> 8, e.r32(p + 0x1a4) >> 8) if p else None)
