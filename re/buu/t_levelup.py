from common import *
from ents import enemies
REC = 0x03001560
e = emu('buu_fight_swforest.ss')
e.w32(REC + 0x24, 2874)   # 1 xp short of level 61
for o in enemies(e):
    e.w32(o + 0x28c, 1 << 10)
print('before', e.read(REC, 0x68).hex())
S = Shots(e, 'lvl')
for f in range(400):
    e.set_keys(KEY.A if f % 8 < 4 else 0)
    e.run(1)
    if f % 50 == 0:
        S.snap()
print('after ', e.read(REC, 0x68).hex())
print('pts', e.r16(REC + 0x2c), 'HP', e.r16(REC + 0x16), e.r16(REC + 0x18), 'EP', e.r16(REC + 0x1a), e.r16(REC + 0x1c))
for k in range(6):
    e.press(KEY.A, 4, 40)
    S.snap()
S.write()
e.save_file(WORK / 'lvl.ss')
