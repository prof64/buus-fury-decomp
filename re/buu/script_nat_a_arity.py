"""Measure the stack effect of natives 0x00..0x57 (and any range given) by running
`push 1001..1008; native n; end` through the game's runner from a fresh save state.
Writes work/buu/script/nat_a_arity.json: {n: {"sp": sp_after or null, "stack": [...]}}"""
import json
import sys
from script_nat_a_lib import *

lo = int(sys.argv[1], 16) if len(sys.argv) > 1 else 0
hi = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0x58
state = sys.argv[3] if len(sys.argv) > 3 else 'buu_snakeway_start.ss'
e = emu(state)
e.run(2)
base = e.save()
res = {}
for n in range(lo, hi):
    e.load(base)
    code = asm(list(range(1001, 1009)) + [('n', n)])
    r = run_code(e, code)
    if r is None:
        res[n] = {'sp': None}
        print(f'{n:02x} BLOCKED/no return')
        continue
    p, sp, st = r
    res[n] = {'sp': sp, 'stack': st}
    print(f'{n:02x} sp={sp} top={[hex(x) for x in st[-3:]]}')
out = WORK / 'script' / f'nat_arity_{lo:02x}_{hi:02x}.json'
out.write_text(json.dumps(res, indent=0))
