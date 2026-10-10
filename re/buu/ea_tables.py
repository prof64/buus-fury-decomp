"""ea_tables.py -- dump the enemy SetAnim(id) table (raw 0x08072670, state 0x080726B8) with each state's vtable slots."""
from common import *

e = emu('buu_fight_swforest.ss')
e.run(2)
seen = {}
for i in range(34):
    n = u8(0x08072670 + i)
    obj = u32(0x080726B8 + 4 * i)
    vt = e.r32(obj) if 0x03000000 <= obj < 0x03008000 else 0
    sl = [(vt + e.r32(vt + 4 * s)) & 0xffffffff for s in range(11)] if vt else []
    print('id %2d raw %3d state %08x vt %08x b4=%3d b5=%3d slots %s' % (
        i, n + 1, obj, vt, e.r8(obj + 4) if vt else -1, e.r8(obj + 5) if vt else -1,
        ' '.join('%08x' % s for s in sl)))
    if vt:
        seen[vt] = sl
