"""Babidi: value of the flag [0x0300302C] (read by Majin Buu's node 0x802B5B1) over time, writers of it,
and Babidi's states, for types 0x19 / 0x1d / 0x04; force 0x1d into its gesture state 0xc6."""
from common import *
from bv_buu_lib import *

for t, forced in ((0x19, None), (0x1d, 0xc6), (0x04, None)):
    e, o = setup(t, dx=90)
    if forced:
        force(e, o, forced)
    w = e.watch(0x0300302C, 0x03003030, 'wc')
    vals = []
    for f in range(400):
        e.w16(REC + 0x16, e.r16(REC + 0x18))
        e.run(1)
        for h in e.hits():
            if h.get('kind') == 'watch':
                vals.append((f, hex(h['new']), 'pc', hex(h['pc'])))
    e.clear(w)
    print(hex(t), 'forced', forced and hex(forced), 'flag writes', vals[:10], 'final', hex(e.r32(0x0300302C)),
          'ctrl', hex(e.r32(o + 0x14)), 'height', e.r16(o + 0x176))
