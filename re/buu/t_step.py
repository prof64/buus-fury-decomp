from common import *
from emutools import cur_pc
e = emu(WORK / 'p2.ss')
target = 0x08009030
for f in range(3000):
    st = e.save()
    fr = e.frame
    b = e.breakpoint(target)
    e.run(1)
    hs = e.hits()
    e.clear(b)
    if hs:
        print('hit at frame', fr, hs[0]['frame'], hex(hs[0]['pc']))
        e.load(st)
        print('after load frame', e.frame)
        seen = set()
        for i in range(3_000_000):
            p = cur_pc(e)
            if 0x08009000 <= p < 0x08009100:
                seen.add(p)
            e.step()
        print(sorted(hex(x) for x in seen)[:20], 'frame now', e.frame)
        break
