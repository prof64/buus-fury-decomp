"""tp.py in.ss world map entrance out.ss [frames_after] -- hijack the next EnterArea call (0x08009030)."""
import sys
from common import *
from emutools import run_to_pc, cur_pc
inp = sys.argv[1]
w, m, ent = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
out = sys.argv[5]
after = int(sys.argv[6]) if len(sys.argv) > 6 else 300
e = emu(inp if (STATES / inp).exists() else WORK / inp)
ok = run_to_pc(e, 0x08009030, mash=KEY.A)
print('hit', ok, hex(cur_pc(e)), 'orig args', e.reg(1), e.reg(2), e.reg(3))
e.set_reg(1, w); e.set_reg(2, m); e.set_reg(3, ent)
S = Shots(e, 'grid')
for i in range(8):
    e.run(after // 8)
    S.snap()
S.write()
e.save_file(WORK / out)
print('area ptr', hex(e.r32(0x03001980)))
