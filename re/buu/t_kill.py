from common import *
from ents import enemies, info
REC = 0x03001560
e = emu('buu_fight_swforest.ss')
print('xp before', e.r32(REC + 0x24), 'zenie', e.r32(0x0300155c))
for o in enemies(e):
    e.w32(o + 0x28c, 1 << 10)
e.watch(REC + 0x24, REC + 0x28, 'w')
for f in range(400):
    e.set_keys(KEY.A if f % 8 < 4 else 0)
    e.run(1)
for h in e.hits():
    print(h['frame'], hex(h['pc']), hex(h['lr']), h['old'], '->', h['new'], [hex(x) for x in h['regs'][:6]])
print('xp after', e.r32(REC + 0x24), 'enemies left', len(enemies(e)))
e.save_file(WORK / 'killed.ss')
