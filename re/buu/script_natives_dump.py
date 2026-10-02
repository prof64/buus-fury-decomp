"""Dump every script native / setter handler (tables 0x08055098 / 0x08055478) to work/buu/script/natives_asm.txt.

Each handler is disassembled until its first return; BL targets are annotated."""
from common import *

NAT, SET = 0x08055098, 0x08055478
(WORK / 'script').mkdir(exist_ok=True)
out = []


def body(a, maxn=90):
    lines = dis(a & ~1, maxn).split('\n')
    res = []
    # follow forward branches: stop at a return only if no pending branch target lies beyond it
    furthest = 0
    for l in lines:
        res.append(l)
        try:
            addr = int(l.split(':')[0], 16)
        except ValueError:
            continue
        if ' b' in l and '#0x' in l and ' bl ' not in l:
            try:
                t = int(l.split('#0x')[-1].split()[0], 16)
                if t > furthest:
                    furthest = t
            except ValueError:
                pass
        if (('pop' in l and 'pc' in l) or 'bx lr' in l or 'bx r' in l) and addr >= furthest:
            break
    return res


for i in range(248):
    a = u32(NAT + 4 * i)
    out.append(f'=== native 0x{i:02x} @ {a:08x}')
    out += body(a)
for i in range(18):
    a = u32(SET + 4 * i)
    out.append(f'=== setter 0x{i:02x} @ {a:08x}')
    out += body(a)
(WORK / 'script' / 'natives_asm.txt').write_text('\n'.join(out) + '\n')
print(len(out))
