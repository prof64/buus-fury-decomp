"""rngsum.py: stdin = seg_rng.py lines 'FRAME N [(fn, caller, arg), ...]' -> per frame the callers (not the rand
wrappers) with counts."""
import ast
import sys
from collections import Counter

for ln in sys.stdin:
    try:
        f, n, rest = ln.split(' ', 2)
        L = ast.literal_eval(rest)
    except Exception:
        continue
    c = Counter((a[1], a[2]) for a in L if a[1] not in ('0x803fad5', '0x803fae7'))
    print(f, n, dict(c))
