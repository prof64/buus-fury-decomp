"""grep native calls in the rhai export of events lo..hi: python3 grep_ev.py REGEX lo hi"""
import re
import sys
from flags_scan import rhai

rx = re.compile(sys.argv[1])
lo, hi = int(sys.argv[2]), int(sys.argv[3])
for i in range(lo, hi + 1):
    t = rhai(f's{i:03d}')
    for line in t.splitlines():
        if rx.search(line) and not line.strip().startswith('//'):
            print(i, line.strip()[:150])
