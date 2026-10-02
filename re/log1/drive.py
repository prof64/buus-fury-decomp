"""Tiny driver: drive.py IN_STATE OUT_STATE SHOT cmd cmd ...
cmd: 'A:2:30' = press key A for 2 frames then wait 30; 'w:60' = wait 60 frames;
'h:RIGHT:40' = hold for 40 frames (released after); 'Ax10' style repeat: 'A:2:30x10'.
'w8:ADDR:VAL', 'w16:ADDR:VAL'.
"""
import sys
from common import *

inp, outp, shot = sys.argv[1:4]
e = emu(inp)
for c in sys.argv[4:]:
    rep = 1
    if 'x' in c.split(':')[-1]:
        c, r = c.rsplit('x', 1)
        rep = int(r)
    for _ in range(rep):
        p = c.split(':')
        if p[0] == 'w':
            e.run(int(p[1]))
        elif p[0] == 'h':
            k = 0
            for n in p[1].split('+'):
                k |= getattr(KEY, n)
            e.hold(k, int(p[2])); e.set_keys(0)
        elif p[0] in ('w8', 'w16', 'w32'):
            getattr(e, p[0])(int(p[1], 0), int(p[2], 0))
        else:
            k = 0
            for n in p[0].split('+'):
                k |= getattr(KEY, n)
            e.press(k, int(p[1]) if len(p) > 1 else 2, int(p[2]) if len(p) > 2 else 10)
if outp != '-':
    e.save_file(STATES / outp)
if shot != '-':
    e.screenshot(SHOTS / shot, scale=1)
print('frame', e.frame)
