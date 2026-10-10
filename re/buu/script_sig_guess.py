"""Rough stack-effect guess for every native from natives_asm.txt (placeholder until the per-native decode lands).
pops = number of 'subs rX, #1' directly followed by a store of the stack count; pushes = 1 if the handler
increments the count ('adds rY, rX, #1' + 'str rY, [r0]'). Writes work/buu/script/sig_guess.json."""
import json
import re
from common import *

txt = (WORK / 'script' / 'natives_asm.txt').read_text().split('\n')
cur = None
res = {}
lines = {}
for l in txt:
    m = re.match(r'=== native 0x([0-9a-f]+)', l)
    if m:
        cur = int(m.group(1), 16)
        lines[cur] = []
        continue
    if l.startswith('=== setter'):
        cur = None
        continue
    if cur is not None:
        lines[cur].append(l)
for n, ls in lines.items():
    ins = [x.split(None, 2)[2] if len(x.split(None, 2)) > 2 else '' for x in ls]
    pops = 0
    pushes = 0
    for i, s in enumerate(ins):
        if re.match(r'subs r\d, #1$', s) and i + 1 < len(ins) and re.match(r'str r\d, \[r\d\]$', ins[i + 1]):
            pops += 1
        if re.match(r'adds r\d, r\d, #1$', s) and any(re.match(r'str r\d, \[r\d\]$', t) for t in ins[i + 1:i + 3]):
            pushes = 1
    res[n] = {'pops': pops, 'pushes': pushes}
(WORK / 'script' / 'sig_guess.json').write_text(json.dumps(res, indent=0))
print(sum(1 for v in res.values() if v['pushes']), 'pushers')
