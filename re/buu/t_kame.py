from common import *
from emutools import catch
from dmglog import DMG_RET
REC = 0x03001560
e = emu('buu_fight_swforest.ss')
e.w8(REC + 0x15, 1)    # select Kamehameha (skill slot 1)
print('skill bytes', e.read(REC + 0x2e, 8).hex(), 'EP', e.r16(REC + 0x1a))
e.watch(REC + 0x2e, REC + 0x3c, 'w')
log = []


def cb(e, pc):
    hit = e.reg(4)
    ad = e.r32(hit + 8)
    log.append((e.frame, e.r8(ad), e.r8(ad + 1), e.r32(hit + 4), round(e.reg(0) / 1024, 2), e.r16(REC + 0x1a)))


catch(e, [DMG_RET], 240, lambda f: KEY.B if f < 120 else 0, cb)
for h in e.hits():
    print('skillbyte write', hex(h['pc']), hex(h['addr'] - REC), h['old'], '->', h['new'])
print('skill bytes', e.read(REC + 0x2e, 8).hex(), 'EP', e.r16(REC + 0x1a))
for l in log:
    print(l)
