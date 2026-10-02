"""For each behaviour-node create function, find the runtime vtable it installs and dump the
interesting virtual functions (enter=vt[0], update=vt[1], exit=vt[2], condition=vt[5])."""
import sys
import capstone
from collections import Counter
from common import *
from vtab import entries
from ai_walk import walk, leaf, leaf_ex, T, N, CONTAINERS

md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)


def create_vtable(fn):
    fn &= ~1
    code = rom()[ra(fn):ra(fn) + 200]
    vts = []
    for ins in md.disasm(code, fn):
        if ins.mnemonic == 'ldr' and 'pc' in ins.op_str:
            off = int(ins.op_str.split('#')[1].rstrip(']'), 16)
            lit = u32(((ins.address + 4) & ~3) + off)
            if 0x08049000 <= lit < 0x08050000:
                vts.append(lit)
        if ins.mnemonic == 'pop' and 'pc' in ins.op_str:
            break
    return vts[-1] if vts else None


def fn_text(f, n=30):
    lines = dis(f & ~1, n).split('\n')
    out = []
    for l in lines:
        out.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l:
            break
    return '\n'.join(out)


if __name__ == '__main__':
    for i in range(N):
        walk(u32(T + 8 * i))
    order = [f for f, n in leaf.most_common()]
    for f in order[:int(sys.argv[1]) if len(sys.argv) > 1 else 40]:
        vt = create_vtable(f)
        print(f'######## create {f:08x} n={leaf[f]} vt={vt and hex(vt)} example def {leaf_ex[f][0]:08x}: ' +
              ' '.join(f'{u32(leaf_ex[f][0] + 4 * k):08x}' for k in range(6)))
        if vt:
            ents = entries(vt, 7)
            for slot in (0, 1, 2, 5):
                print(f'--- vt[{slot}] {ents[slot]:08x}')
                print(fn_text(ents[slot], 45))
