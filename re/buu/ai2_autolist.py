"""ai2_autolist.py -- print ai_decode.auto() for every reachable node type not in the registry."""
import ai2_boot  # noqa: F401
import json
from collections import Counter, defaultdict
from common import *
from ai2_collect import walk_state, ET
from ai_decode import REGISTRY, auto
from ai2_triage import NAMES

nodes = Counter()
types = defaultdict(set)
for t in range(0x96):
    walk_state(u32(ET + 28 * t + 8), set(), nodes, types, t)
for f, n in nodes.most_common():
    if f in REGISTRY:
        continue
    a = auto(f)
    users = sorted({NAMES.get(t) for t in types[f]} - {None})[:5]
    print(f'{f:08x} n={n} {users}')
    print('   ', json.dumps({k: v for k, v in a.items() if k not in ('vtable', 'alloc', 'singleton')}))
