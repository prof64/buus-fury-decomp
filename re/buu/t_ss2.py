from common import *
REC = 0x03001560
PLAYER = 0x03001c34
e = emu('buu_swforest3.ss')
e.w8(REC + 0xc, 1)
e.w8(REC + 0xd, 0x14)
e.w8(REC + 0x15, 0)
p = e.r32(PLAYER)
e.watch(p + 0x1b0, p + 0x1c0, 'w')
e.watch(p + 0x238, p + 0x240, 'w')
e.watch(REC + 0x1e, REC + 0x20, 'w')
e.press(KEY.B, 3, 150)
seen = set()
for h in e.hits():
    k = (h['pc'], h['addr'])
    if k in seen:
        continue
    seen.add(k)
    print(hex(h['pc']), hex(h['lr']), hex(h['addr'] - REC), h['old'], '->', h['new'], [hex(x) for x in h['regs'][:8]])
