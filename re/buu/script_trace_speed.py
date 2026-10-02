"""Measure single-step speed of the emulator (helper for script_trace.py design)."""
import time
from common import *
from emutools import cur_pc

e = emu('buu_snakeway_start.ss')
e.run(5)
t = time.time()
n = 0
start = e.frame
while e.frame == start:
    cur_pc(e)
    e.step()
    n += 1
dt = time.time() - t
print('instructions in one frame', n, 'seconds', round(dt, 3), 'per step us', round(dt / n * 1e6, 2))
