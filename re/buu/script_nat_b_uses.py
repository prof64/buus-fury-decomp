"""script_nat_b_uses.py lo hi -- usage census of natives lo..hi over all real scripts of the script table
(blob kind byte 2 == 0 raw / 4 compressed). For each native: number of call sites, scripts using it, and
example constant argument tuples (when the preceding instructions are plain pushes).
Writes work/buu/script/nat_b_uses.json."""
import json
import sys
from collections import defaultdict
from common import *
from scriptdis import dis_script, script_bytes, SCRIPTS

lo = int(sys.argv[1], 16)
hi = int(sys.argv[2], 16)
pops = {}
try:
    st = json.loads((WORK / 'script' / 'nat_b_stack.json').read_text())
    for k, v in st.items():
        if v['paths']:
            pops[int(k)] = v['paths'][0][0]
except Exception:
    pass

uses = defaultdict(int)
scripts = defaultdict(set)
examples = defaultdict(list)
for idx in range(522):
    hdr = u32(SCRIPTS + 4 * idx)
    blob = u32(hdr)
    if u8(blob + 2) not in (0, 4):
        continue
    data, base = script_bytes(idx)
    lines = dis_script(base, data=data, stop_at_end=False)
    # stop at first unknown op
    vals = []
    for a, s in lines:
        if s.startswith('??'):
            break
        if s.startswith('push ') and not s.startswith('pushn'):
            vals.append(int(s.split()[1]))
            continue
        if s.startswith('pushn'):
            vals += json.loads(s[6:])
            continue
        if s.startswith('native'):
            n = int(s.split()[1], 16)
            if lo <= n <= hi:
                uses[n] += 1
                scripts[n].add(idx)
                p = pops.get(n, 0)
                args = vals[-p:] if p and len(vals) >= p else ('?' if p else [])
                if len(examples[n]) < 12:
                    examples[n].append((idx, hex(a), args))
            vals = []
            continue
        vals = []

out = {}
for n in range(lo, hi + 1):
    out[n] = {'uses': uses[n], 'scripts': sorted(scripts[n])[:40], 'nscripts': len(scripts[n]),
              'examples': examples[n]}
    print(f'0x{n:02x} uses={uses[n]:4d} scripts={len(scripts[n]):3d} ex={examples[n][:6]}')
(WORK / 'script' / 'nat_b_uses.json').write_text(json.dumps(out, indent=1))
