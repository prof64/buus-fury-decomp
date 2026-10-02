"""stackat.py state addr frames keyspec [maxhits] -- when addr executes, dump regs + stack words that look like return addresses"""
import sys
from common import *
from emutools import catch
from combat import keyfn_from
st = sys.argv[1]
e = emu(st if (STATES / st).exists() else WORK / st)
addr = int(sys.argv[2], 16)
frames = int(sys.argv[3])
kf = keyfn_from(sys.argv[4]) if len(sys.argv) > 4 else (lambda f: 0)
mx = int(sys.argv[5]) if len(sys.argv) > 5 else 3
REC = 0x03001560
cnt = [0]


def cb(e, pc):
    if cnt[0] >= mx:
        return
    cnt[0] += 1
    regs = [e.reg(i) for i in range(16)]
    print('hit frame', e.frame, ' '.join(f'r{i}={regs[i]:08x}' for i in range(16)))
    sp = regs[13]
    rets = []
    for k in range(64):
        w = e.r32(sp + 4 * k)
        if 0x08000000 < w < 0x08060000 and w & 1:
            rets.append(f'sp+{4*k:#x}:{w:08x}')
    print('   ret-like:', ' '.join(rets))


if len(sys.argv) > 6:
    e.w16(REC + 0x1a, 0)
catch(e, [addr], frames, kf, cb)
