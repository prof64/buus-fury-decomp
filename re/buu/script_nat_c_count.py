"""Native use counts over ALL bytecode in the resource table: every event-graph node of kind 0 (raw), 4 (LZ),
and the argument bytecode of kinds 2/6 (at blob+8); plus supply use-scripts and equipment effect scripts.
Writes work/buu/script/native_uses_c.json {native: {"uses": n, "scripts": [...]}}."""
import json
from collections import defaultdict
from common import *
from scriptdis import varint
from script_nat_c_graph import walk, TABLE, N
from decomp import decompress


def reach(code):
    seen = {}
    todo = [0]
    while todo:
        i = todo.pop()
        while 0 <= i < len(code) and i not in seen:
            a = i
            op = code[i]; i += 1
            arg = None
            try:
                if op in (0, 2, 3) or 0x15 <= op <= 0x1a:
                    arg = code[i]; i += 1
                elif op == 1:
                    arg, i = varint(code, i)
                elif op == 0x1d:
                    n = code[i]; i += 1
                    for _ in range(n):
                        _, i = varint(code, i)
                elif op in (0x12, 0x13, 0x1c):
                    off = (code[i] << 8) | code[i + 1]
                    arg = (a + 1 - off) if op == 0x1c else (a + 1 + off)
                    i += 2
                elif op > 0x1e:
                    break
            except IndexError:
                break
            seen[a] = (op, arg)
            if op == 0x11:
                break
            if op == 0x12:
                i = arg
            elif op in (0x13, 0x1c):
                todo.append(arg)
    return seen


uses = defaultdict(int)
where = defaultdict(set)


def count(code, tag):
    for a, (op, arg) in reach(code).items():
        if op == 2:
            uses[arg] += 1
            where[arg].add(tag)


for i in range(N):
    h = u32(TABLE + 4 * i)
    for s, w in walk(h).items():
        if not (w >= 0x10000 and 0x08000000 <= w < 0x08800000):
            continue
        k = u8(w + 2)
        if k == 0:
            count(rom()[ra(w + 3):ra(w + 3) + 0x4000], f'#{i}')
        elif k == 4:
            count(decompress(w + 4), f'#{i}')
        elif k in (2, 6):
            count(rom()[ra(w + 8):ra(w + 8) + 0x400], f'#{i}')
# supplies (use script +0x30) and equipment (effect script +0x28)
for j in range(83):
    p = u32(0x083BD2F8 + 0x34 * j + 0x30)
    if 0x08000000 <= p < 0x08800000:
        count(rom()[ra(p):ra(p) + 0x400], f'supply{j}')
for j in range(162):
    p = u32(0x086F82EC + 0x2C * j + 0x28)
    if 0x08000000 <= p < 0x08800000:
        count(rom()[ra(p):ra(p) + 0x400], f'equip{j}')
out = {f'0x{n:02x}': {'uses': uses[n], 'where': sorted(where[n])[:20]} for n in range(256) if uses[n]}
(WORK / 'script' / 'native_uses_c.json').write_text(json.dumps(out, indent=1))
for n in range(0xa8, 0xf8):
    print(f'0x{n:02x}', uses[n], sorted(where[n])[:8])
