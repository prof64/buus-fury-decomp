"""bv_buu_dis.py addr [addr..] [-vt vtable n] : full-function disassembly helper (output to stdout).
'-vt 0x0804xxxx 8' also disassembles each vtable slot."""
import sys
from common import *
from vtab import entries
from bv_buu_nodes import dis_fn

args = sys.argv[1:]
i = 0
while i < len(args):
    if args[i] == '-vt':
        vt = int(args[i + 1], 16); n = int(args[i + 2])
        es = entries(vt, n)
        print(f'#### vtable {vt:#x}:', ' '.join(hex(x) for x in es))
        for k, x in enumerate(es):
            if 0x08000000 <= x < 0x08100000 and x not in es[:k]:
                print(f'---- vt[{k}] {x:#x}')
                print(dis_fn(x))
        i += 3
        continue
    a = int(args[i], 16)
    print(f'#### {a:#x}')
    print(dis_fn(a))
    i += 1
