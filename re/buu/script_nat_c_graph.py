"""Enumerate the event-graph nodes of every resource-table entry (0x0879B704).

Header = array of u32 "step" words. Step word < 0x10000 = END (value = result); otherwise a pointer to a node
blob {u16 next_step, u8 kind, u8 arg, payload...}. Kind handlers: table 0x080728AC (see
engine/annotations/script_resource_kinds.json)."""
import json
import sys
from collections import Counter, defaultdict
from common import *
from strtab import ustr

TABLE = 0x0879B704
N = 522


def isptr(v):
    return 0x08000000 <= v < 0x08800000


def walk(h):
    """BFS over step indices reachable via node.next (script/choice nodes may branch: we also include every
    header word up to the highest index referenced)."""
    steps = {}
    todo = [0]
    maxidx = 0
    while todo:
        s = todo.pop()
        if s in steps or s > 64:
            continue
        w = u32(h + 4 * s)
        steps[s] = w
        maxidx = max(maxidx, s)
        if w >= 0x10000 and isptr(w):
            nxt = u16(w)
            todo.append(nxt)
    # scripts/branching nodes can return other step indices: include all words up to the max seen + scan on
    s = 0
    while s <= maxidx + 8:
        w = u32(h + 4 * s)
        if s not in steps:
            if w < 0x10000 or (isptr(w) and u8(w + 2) <= 7 and u16(w) < 64):
                steps.setdefault(-1 - s, w)  # unreached candidate (negative key = not reached by next)
            else:
                break
        s += 1
    return steps


if __name__ == '__main__':
    kinds = Counter()
    kinds_reached = Counter()
    samples = defaultdict(list)
    for i in range(N):
        h = u32(TABLE + 4 * i)
        st = walk(h)
        for s, w in st.items():
            if w >= 0x10000 and isptr(w):
                k = u8(w + 2)
                kinds[k] += 1
                if s >= 0:
                    kinds_reached[k] += 1
                if len(samples[k]) < 4:
                    extra = ''
                    if k in (1, 3):
                        extra = ustr(w + 4)[:80]
                    samples[k].append((i, s, hex(w), rom()[ra(w):ra(w) + 16].hex(), extra))
    print('all nodes by kind', dict(kinds))
    print('reached via next', dict(kinds_reached))
    for k, v in sorted(samples.items()):
        print('kind', k)
        for x in v:
            print('   ', x)
