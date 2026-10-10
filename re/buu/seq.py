"""seq.py in out "KEY:hold:wait,..."  -- run a key sequence; snap after each step. '.' = snap only"""
import sys
from common import *
inp, out = sys.argv[1], sys.argv[2]
e = emu(inp if (STATES / inp).exists() else WORK / inp)
S = Shots(e, 'grid')
for step in sys.argv[3].split(','):
    parts = step.split(':')
    k = 0
    if parts[0] not in ('', '-'):
        for ch in parts[0].split('+'):
            k |= getattr(KEY, ch)
    hold = int(parts[1]) if len(parts) > 1 else 4
    wait = int(parts[2]) if len(parts) > 2 else 20
    nosnap = len(parts) > 3
    e.press(k, hold, wait)
    if not nosnap:
        S.snap()
S.write()
if out != '-':
    e.save_file(WORK / out)
