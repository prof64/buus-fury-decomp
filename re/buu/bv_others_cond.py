"""Who reads an enemy spawn record's condition word (rec+4)?  Warp into SW forest with a read watch on the
first Majin Fighter record +4..+8, print PCs."""
from common import *
from spawns import scan
from warp import warp

sw = [s for s in scan() if (s['world'], s['map']) == (8, 3) and s['type'] == 0x44]
rec = int(sw[0]['record'], 16)
print('rec', hex(rec), 'cond', sw[0]['cond'])
e = emu('buu_snakeway_start.ss')
e.watch(rec + 4, rec + 8, 'r')
warp(e, 8, 3, 100, 100)
pcs = {}
for h in e.hits():
    pcs.setdefault(h['pc'], h)
for pc, h in pcs.items():
    print(hex(pc), [hex(x) for x in h['regs'][:8]])
