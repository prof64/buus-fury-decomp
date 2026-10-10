"""Kill all Majin Fighters: who reads type-entry bytes, which records gain XP, what drops."""
from common import *
from ents import enemies
REC0 = 0x03001560
e = emu('buu_fight_swforest.ss')
xp0 = [e.r32(REC0 + 0x68 * i + 0x24) for i in range(9)]
z0 = e.r32(0x0300155c)
for o in enemies(e):
    e.w32(o + 0x28c, 1 << 10)
e.watch(0x087119e4 + 0xc, 0x087119e4 + 0x1c, 'r')
for f in range(600):
    e.set_keys(KEY.A if f % 8 < 4 else 0)
    e.w16(REC0 + 0x16, 135)
    e.run(1)
seen = set()
for h in e.hits():
    k = (h['pc'], h['addr'] - 0x087119e4)
    if k not in seen:
        seen.add(k)
        print('read', hex(h['pc']), 'off', hex(k[1]), 'lr', hex(h['lr']))
print('xp delta', [e.r32(REC0 + 0x68 * i + 0x24) - xp0[i] for i in range(9)], 'zenie', z0, '->', e.r32(0x0300155c))
print('enemies left', len(enemies(e)))
S = Shots(e, 'kill3'); S.snap(); S.write()
e.save_file(WORK / 'kill3.ss')
