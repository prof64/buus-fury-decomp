"""tp2.py in.ss world map entrance out.ss [frames_after] [op] -- teleport during gameplay by
invoking the bytecode 'change area' op handler (0x08002184, fade-in variant) with a fake VM stack."""
import sys
from common import *
from emutools import call


def teleport(e, w, m, ent, op=0x08002184, x=0, y=0):
    vm = 0x0203FF00
    vals = [w, m, ent, x, y]
    e.w32(vm, len(vals))
    for i, v in enumerate(vals):
        e.w32(vm + 4 + 4 * i, v)
    return call(e, op, (vm, 0, 0, 0))


if __name__ == '__main__':
    inp = sys.argv[1]
    w, m, ent = int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
    out = sys.argv[5]
    after = int(sys.argv[6]) if len(sys.argv) > 6 else 240
    op = int(sys.argv[7], 16) if len(sys.argv) > 7 else 0x08002184
    e = emu(inp if (STATES / inp).exists() else WORK / inp)
    teleport(e, w, m, ent, op)
    S = Shots(e, 'grid')
    for i in range(8):
        e.run(after // 8)
        S.snap()
    S.write()
    e.save_file(WORK / out)
    print('area ptr', hex(e.r32(0x03001980)))
