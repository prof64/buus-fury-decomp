"""Automatic layout analysis of enemy behaviour-tree node create functions.

A behaviour node definition in ROM is an array of 32-bit words whose first word is the node's
create function (thumb). The create function receives r0 = definition pointer. It reads parameters
with `ldr rX, [r0, #off]` (before r0 is clobbered) and creates children by loading a pointer and
calling its first word. This script tracks which register holds the def pointer and records the
offsets read, flagging those used as child-definition pointers.
"""
import capstone
from common import *

md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)


def analyze(fn, maxins=80):
    fn &= ~1
    code = rom()[ra(fn):ra(fn) + maxins * 4]
    defregs = {'r0'}
    reads = {}       # off -> kind ('param' | 'child' | 'ptr')
    loaded = {}      # reg -> off loaded from def
    for ins in md.disasm(code, fn):
        m, ops = ins.mnemonic, ins.op_str
        parts = [p.strip() for p in ops.replace('[', '').replace(']', '').split(',')]
        if m in ('ldr', 'ldrh', 'ldrb', 'ldrsh', 'ldrsb') and len(parts) >= 2 and parts[1] in defregs and '#' in ops:
            off = int(parts[2].lstrip('#'), 16) if len(parts) > 2 else 0
            reads.setdefault(off, 'param' if m == 'ldr' else m)
            loaded[parts[0]] = off
            if parts[0] in defregs and parts[0] != parts[1]:
                defregs.discard(parts[0])
            elif parts[0] in defregs:
                defregs.discard(parts[0])
            continue
        if m == 'ldr' and len(parts) >= 2 and parts[1] in defregs and len(parts) == 2:
            reads.setdefault(0, 'param')
            loaded[parts[0]] = 0
        if m == 'adds' and len(parts) == 3 and parts[1] in defregs and parts[2] == '#0':
            defregs.add(parts[0])
            continue
        # child pattern: ldr r2, [rX] where rX was loaded from def
        if m == 'ldr' and len(parts) == 2 and parts[1] in loaded and loaded[parts[1]] is not None:
            reads[loaded[parts[1]]] = 'child'
        if m in ('pop',) and 'pc' in ops:
            break
        if m == 'bx':
            break
        # clobber tracking
        if parts and parts[0].startswith('r') and m not in ('cmp', 'str', 'strh', 'strb', 'stm', 'push', 'bl', 'b') and not m.startswith('b'):
            dst = parts[0].rstrip('!')
            if dst in defregs and not (m == 'adds' and len(parts) == 3 and parts[1] == dst):
                defregs.discard(dst)
            if dst in loaded and not (m.startswith('ldr') and parts[1] in defregs):
                pass
        if m == 'bl':
            # r0-r3 clobbered
            for r in ('r0', 'r1', 'r2', 'r3'):
                defregs.discard(r)
    return reads


if __name__ == '__main__':
    import sys
    for s in sys.argv[1:]:
        print(s, analyze(int(s, 16)))
