"""Dump enter/input/update/exit functions of each skill's player-state class to work/buu/atk_states.txt"""
from common import *
from vtab import entries

VTS = {1: 0x0804de7c, 2: 0x0804d05c, 3: 0x0804e244, 4: 0x0804df18, 5: 0x0804d9b0, 6: 0x0804e2d8, 7: 0x0804dc14,
       8: 0x0804dc38, 9: 0x0804dc5c, 10: 0x0804d9d4, 11: 0x0804e088, 13: 0x0804dc80, 15: 0x0804d9f8, 16: 0x0804da1c,
       17: 0x0804d080, 18: 0x0804d410, 23: 0x0804dfe8}
GENERIC = {0x0804e4e1, 0x0804fead, 0x0800de61, 0x0800de6d}


def full(f, n=200):
    out = []
    seen_ret = False
    for i, l in enumerate(dis(f & ~1, n).split('\n')):
        if i > 0 and seen_ret and 'push' in l and 'lr' in l:
            break
        out.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l:
            seen_ret = True
    return '\n'.join(out)


lines = []
for sk, vt in VTS.items():
    lines.append(f'################ skill {sk} vt {vt:08x}')
    for slot, fn in enumerate(entries(vt, 5)):
        lines.append(f'--- vt[{slot}] {fn:08x}' + (' (generic)' if fn in GENERIC else ''))
        if fn not in GENERIC:
            lines.append(full(fn))
(WORK / 'atk_states.txt').write_text('\n'.join(lines))
print(len(lines))
