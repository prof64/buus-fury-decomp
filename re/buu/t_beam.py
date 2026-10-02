import sys
from common import *
from combat import setup, pin, REC
from emutools import catch
from dmglog import DMG_RET
sk = int(sys.argv[1], 0) if len(sys.argv) > 1 else 0xf
st = sys.argv[2] if len(sys.argv) > 2 else 'buu_fight_swforest.ss'
hold = int(sys.argv[3]) if len(sys.argv) > 3 else 100
dist = int(sys.argv[4]) if len(sys.argv) > 4 else 70
e = emu(st)
e.w8(REC + 0xc, 1); e.w8(REC + 0xd, sk); e.w8(REC + 0x15, 0)
e.w16(REC + 0x1c, 999); e.w16(REC + 0x1a, 999)
p, en = setup(e)
e.w8(p + 0x10, 0)
S = Shots(e, 'beam')
log = []


def cb(e, pc):
    hit = e.reg(4)
    ad = e.r32(hit + 8)
    log.append((e.frame, 'P' if e.r32(hit + 0xc) == p else 'E', e.r8(ad), e.r8(ad + 1), e.r32(hit + 4), round(e.reg(0) / 1024, 2)))


for f in range(220):
    pin(e, en[0], p, 0, dist)
    e.w16(REC + 0x16, e.r16(REC + 0x18))
    catch(e, [DMG_RET], 1, lambda _f, f=f: KEY.B if f < hold else 0, cb)
    if f % 14 == 5:
        S.snap()
S.write()
print('enemy hp', e.r32(en[0] + 0x28c) / 1024, 'EP', e.r16(REC + 0x1a))
for l in log:
    print(l)
