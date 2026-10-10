"""Dump every reachable AI state tree for the Buu/Babidi group -> work/buu/boss_buu_trees.txt."""
import sys
from common import *
from ai_decode import walk_states, initial_state, state_root, decode, pretty, auto, REGISTRY
import json

TYPES = [0x1a, 0x1b, 0x1c, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x3c, 0x3d, 0x04, 0x19, 0x1d]
ENEMY = 0x08711274
out = []
seen = set()
for t in TYPES:
    b = ENEMY + 28 * t
    out.append(f'######## type {t:#x}  exp {u32(b)} hp {u32(b+4)} init {u32(b+8):#x} raw ' +
               ' '.join(f'{u8(b+k):02x}' for k in range(12, 28)))
    for s in walk_states(initial_state(t)):
        tag = ' (dup)' if s in seen else ''
        out.append(f'## state {s:#x}{tag}')
        if s in seen:
            continue
        seen.add(s)
        out += pretty(decode(state_root(s)))
(WORK / 'boss_buu_trees.txt').write_text('\n'.join(out))
print(len(out), 'lines')
if len(sys.argv) > 1:
    for a in sys.argv[1:]:
        print(a, json.dumps(auto(int(a, 16)), indent=1))
