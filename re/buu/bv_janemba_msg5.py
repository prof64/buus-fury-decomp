"""Which world objects answer message 5 (def of gfx 0x99) in Janemba's 'wait' node 0x080398C5?
Steps the update frame and records (object, vtable, result) for every vfunc+0x24 call made by
0x08011CCA (object in r0 at the bl 0x08011CF4/0x08011D22-ish, result in r0 afterwards)."""
import sys
from common import *
from emutools import catch
from ents import TYPE_TAB

e = emu(WORK / (sys.argv[1] if len(sys.argv) > 1 else 'jan_end_33.ss'))
res = []
cur = {}


def cb(e, pc):
    if pc in (0x08011CF4, 0x08011D22):
        cur['o'] = e.reg(0)
    elif pc in (0x08011CF8, 0x08011D26):
        o = cur.get('o')
        if e.reg(0):
            t = (e.r32(o + 0x224) - TYPE_TAB) // 28
            res.append((hex(o), hex(e.r32(o)), 'def', hex(e.r32(o + 0x58)), 'type?', t))
    elif pc == 0x080398A4:
        res.append('--- update')


print(dis(0x08011D1A, 20))
catch(e, [0x080398A4, 0x08011CF4, 0x08011CF8, 0x08011D22, 0x08011D26], 2, cb=cb)
for r in res:
    print(r)
