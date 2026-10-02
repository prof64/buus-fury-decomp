"""ai2_triage.py -- one-line-per-node overview of every reachable node create fn: users (enemy names),
runtime vtable kind, literals and bl targets of enter/update/cond, to classify rare node types quickly."""
import ai2_boot  # noqa: F401
import json
import re
import sys
from collections import Counter, defaultdict
from common import *
from vtab import entries
from ai2_collect import walk_state, ET
from ai2_inspect import runtime_vtable, full, COMMON

NAMES = {e['type']: e['name'] for e in json.load(open(
    '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/data/buu/enemies.json'))['enemies']}


def calls_lits(fn):
    txt = full(fn, 160)
    bls = sorted(set(re.findall(r'bl #0x([0-9a-f]+)', txt)))
    ls = sorted(set(re.findall(r'=0x([0-9a-f]+)', txt)))
    return len(txt.split('\n')), bls, ls


if __name__ == '__main__':
    skip = {int(x, 16) for x in sys.argv[1:]}
    nodes = Counter()
    types = defaultdict(set)
    for t in range(0x96):
        walk_state(u32(ET + 28 * t + 8), set(), nodes, types, t)
    for f, n in nodes.most_common():
        if f in skip:
            continue
        users = sorted(types[f])
        print(f'#### {f:08x} n={n} users={[(t, NAMES.get(t)) for t in users[:8]]}')
        vt, how = runtime_vtable(f)
        print('   vt', vt and hex(vt), how)
        if vt and 0x08000000 <= vt < 0x08800000:
            ents = entries(vt, 8)
            for sl in (0, 1, 5):
                fn = ents[sl]
                if fn in COMMON:
                    print(f'   vt[{sl}] {fn:08x} {COMMON[fn]}')
                    continue
                ln, bls, ls = calls_lits(fn)
                print(f'   vt[{sl}] {fn:08x} len={ln} bl={bls} lit={ls}')
