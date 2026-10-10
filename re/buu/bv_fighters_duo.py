"""Spopovich (0x6f) + Yamu (0x93) together: Spopovich grab (state 0x22e/0x22f) + Yamu EP drain (state 0x334).
Two SW-forest records patched; player kept next to Spopovich; log both AI state changes, player EP, player
controller (0x03001b00 = held)."""
from bv_fighters_lib import *
from forcespawn import spawn_records, own_record, SW, etype
from warp import warp
from ents import enemies

e = emu('buu_snakeway_start.ss')
e.w16(REC + 0x16, 9999); e.w16(REC + 0x18, 9999)
e.w16(REC + 0x1a, 400); e.w16(REC + 0x1c, 400)
sw = [s for s in spawn_records() if (s['world'], s['map']) == SW and s['type'] == 0x44]
for rec_s, t, dx in ((sw[0], 0x6f, 40), (sw[1], 0x93, 90)):
    tgt = int(rec_s['record'], 16)
    src = own_record(t)
    for off in (0x10, 0x14, 0x18, 0x1c):
        e.w32(tgt + off, u32(src + off))
    e.w32(tgt + 8, t)
x0, y0 = s16(int(sw[0]['record'], 16) + 0xc), s16(int(sw[0]['record'], 16) + 0xe)
for k, s in enumerate(sw):
    r = int(s['record'], 16)
    if k >= 2:
        e.w16(r + 0xc, 0x7000); e.w16(r + 0xe, 0x7000)
    else:
        e.w16(r + 0xc, x0 + (30 if k == 0 else 100)); e.w16(r + 0xe, y0)
warp(e, SW[0], SW[1], x0, y0)
obs = {etype(e, o): o for o in enemies(e)}
spo, yam = obs.get(0x6f), obs.get(0x93)
print('spopovich', hex(spo or 0), 'yamu', hex(yam or 0))
p = e.r32(PLAYER)
log = []
lastk = None


def cb(e, pc):
    who = {spo + 0x238: 'S', yam + 0x238: 'Y'}.get(e.reg(0))
    if who:
        log.append((e.frame, who, hex(e.reg(1))))


sx, sy = pos(e, spo)
setpos(e, p, sx - 6, sy)
set_state(e, spo, 0x22e)
f0 = e.frame
for f in range(3200):
    e.w16(REC + 0x16, 9999)
    if False:
        sx, sy = pos(e, spo)
        if e.r32(p + 0x14) != 0x03001b00:
            setpos(e, p, sx - 10, sy)
    catch(e, [REPLACE], 1, lambda _: 0, cb)
    k = (e.r16(REC + 0x1a), hex(e.r32(p + 0x14)))
    if k != lastk:
        log.append((e.frame, 'P', k))
        lastk = k
for x in log:
    if x[1] != 'P' or len(log) < 400:
        print(x[0] - f0, x[1], x[2])
eps = [(x[0] - f0, x[2][0]) for x in log if x[1] == 'P']
print('EP samples', eps[:5], '...', eps[-5:])
