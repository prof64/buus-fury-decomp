"""npc_re.py: disassembly helpers for the NPC behaviour RE.
   npc_re.py fn ADDR [n]      disassemble n thumb instructions at ADDR
   npc_re.py vt ADDR [n]      list self-relative vtable entries"""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 're' / 'buu'))
from common import *  # noqa


def vt(a, n=12):
    r = []
    for i in range(n):
        off = s32(a + 4 * i)
        r.append((a + off) & 0xffffffff)
    return r


if __name__ == '__main__':
    cmd = sys.argv[1]
    for arg in sys.argv[2:]:
        if ':' in arg:
            a, n = arg.split(':')
            a, n = int(a, 16), int(n)
        else:
            a, n = int(arg, 16), 80
        print(f'==== {cmd} {a:08x}')
        if cmd == 'fn':
            print(dis(a & ~1, n))
        else:
            for i, f in enumerate(vt(a, n)):
                print(f'+{4*i:#04x} {f:08x}')
