"""chg.py: stdin lines 'FRAME [(name, hex), ...]' (segpeek.py) -> for every watched value except the last one, the
frames where it changes (the last value is printed alongside as context)."""
import ast
import sys

prev = None
for ln in sys.stdin:
    f, rest = ln.split(' ', 1)
    try:
        vals = ast.literal_eval(rest)
    except Exception:
        continue
    cur = [v for _, v in vals[:-1]]
    if prev is not None:
        for k, (a, b) in enumerate(zip(prev, cur)):
            if a != b:
                print(f, vals[k][0], a, '->', b, 'ctx', vals[-1][1])
    prev = cur
