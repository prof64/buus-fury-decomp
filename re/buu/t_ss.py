"""Grant + use Super Saiyan on Goku; observe form, stats, gauges."""
from common import *
REC = 0x03001560
PLAYER = 0x03001c34
e = emu('buu_swforest3.ss')
e.w8(REC + 0xc, 1)
e.w8(REC + 0xd, 0x14)
e.w8(REC + 0x15, 0)


def show(tag):
    p = e.r32(PLAYER)
    rb = e.read(REC, 0x68)
    print(tag, 'form', rb[0x14], 'HP', e.r16(REC + 0x16), 'EP', e.r16(REC + 0x1a), '1e', e.r16(REC + 0x1e), '20', e.r16(REC + 0x20),
          'objSTR..', [e.r16(p + 0x1b0 + 2 * i) for i in range(4)], 'speed', e.r32(p + 0x238), 'vt', hex(e.r32(p)),
          'state', hex(e.r32(p + 0x14)))


show('before')
e.press(KEY.B, 3, 60)
show('after B')
for i in range(6):
    e.run(120)
    show(f't+{(i+1)*2}s')
e.save_file(WORK / 'ss.ss')
S = Shots(e, 'ss'); S.snap(); S.write()
