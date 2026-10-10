"""script_cat_sites.py -- for every BL into the VM (0x08004038 interp, 0x08004098 runner, 0x08012588
run_event, 0x0801246e event ctor), print literal-pool values loaded in the preceding 0x60 bytes (resolving
event-table headers to indices) and the next few instructions (to see whether the result is read)."""
import re
from common import *
from script_cat_callers import bl_sites

SCRIPTS = 0x0879B704
HDR = {u32(SCRIPTS + 4 * i): i for i in range(522)}
T = {0x08004038: 'interp', 0x08004098: 'runner', 0x08012588: 'run_event', 0x0801246e: 'event_new'}
for site, t in bl_sites(list(T)):
    lines = dis(site - 0x60, 0x30 + 4).split('\n')
    lits = []
    for l in lines:
        m = re.search(r'; =0x([0-9a-f]{8})', l)
        if m:
            v = int(m.group(1), 16)
            lits.append(f'{v:08x}' + (f'(#{HDR[v]})' if v in HDR else ''))
    after = [l.split(None, 2)[2] for l in dis(site + 4, 4).split('\n') if l.strip()]
    print(f'{site:08x} {T[t]:9s} lits={lits[-6:]} after={after}')
