"""Collect node create-functions reachable from every enemy type's initial AI state (follows children,
SubState and GotoState edges). Unknown node layouts come from ai_nodes.analyze heuristics."""
import ai2_boot  # noqa: F401
from collections import Counter, defaultdict
from common import *
from ai_nodes import analyze

T = 0x0870F514
ET = 0x08711274
CONT = {0x08018f91, 0x080191c7, 0x080191e9, 0x0801920f}
STATE_EDGE = {0x0801890b, 0x08019285}  # param = state id


def state_def(s):
    return u32(T + 8 * s)


def walk_state(s, states, nodes, types, fromtype):
    if s in states or s > 0x400:
        return
    states.add(s)
    walk_node(state_def(s), states, nodes, types, fromtype, set())


def walk_node(d, states, nodes, types, fromtype, seen):
    if d in seen or not (0x08000000 <= d < 0x08800000):
        return
    seen.add(d)
    f = u32(d)
    nodes[f] += 1
    types[f].add(fromtype)
    if f in CONT:
        n = u32(d + 0xc)
        for k in range(min(n, 40)):
            walk_node(u32(d + 0x10 + 4 * k), states, nodes, types, fromtype, seen)
        return
    if f in STATE_EDGE:
        walk_state(u32(d + 0xc), states, nodes, types, fromtype)
        return
    for off, kind in analyze(f).items():
        if kind == 'child':
            walk_node(u32(d + off), states, nodes, types, fromtype, seen)


if __name__ == '__main__':
    nodes = Counter()
    types = defaultdict(set)
    allstates = set()
    for t in range(0x96):
        st = set()
        walk_state(u32(ET + 28 * t + 8), st, nodes, types, t)
        allstates |= st
    print('states', len(allstates))
    for f, n in nodes.most_common():
        print(f'{f:08x} n={n:4d} enemies={len(types[f]):3d} layout={analyze(f)} eg={sorted(types[f])[:6]}')
