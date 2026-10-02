"""Compare the VM loot simulation with the real game: kill a Majin Fighter (loot level 75) many times with
randomised RNG state and count the drop natives the game executes (0x84 zenie, 0x85 supply, 0x86 equip)."""
import random
from collections import Counter
from common import *
from emutools import catch
from ents import enemies
from loot import simulate

NAT = {0x0800253C: 'zenie', 0x08002556: 'supply', 0x080025A4: 'equip'}
REC = 0x03001560
rnd = random.Random(7)
counts = Counter()
kills = 0
base = emu('buu_fight_swforest.ss')
st0 = base.save()
for trial in range(500):
    e = base
    e.load(st0)
    e.w32(0x03004a5c, rnd.getrandbits(32)); e.w32(0x03004a60, rnd.getrandbits(32))
    en = enemies(e)
    o = en[0]
    p = e.r32(0x03001c34)
    for x in en[1:]:
        pass
    e.w32(o + 0x28c, 1 << 10)
    e.w32(REC + 0x24, 60000000)
    e.w8(p + 0x10, 3)
    e.w32(o + 0x1a0, e.r32(p + 0x1a0) + (18 << 8)); e.w32(o + 0x1a4, e.r32(p + 0x1a4))
    got = Counter()

    def cb(e, pc):
        got[NAT[pc]] += 1

    catch(e, list(NAT), 160, lambda f: KEY.A if f < 2 else 0, cb)
    if len(enemies(e)) < len(en):
        kills += 1
        counts.update(got)
        counts['kills_with_drops'] += 1 if sum(got.values()) else 0
print('kills', kills, dict(counts))
print('game per kill:', {k: round(v / kills, 3) for k, v in counts.items()})
sim = simulate(75, 132, n=20000)  # Majin Fighter: run_script(497, 75, 132)
agg = Counter()
for k, v in sim['per_kill'].items():
    agg[k.split(':')[0]] += v
print('sim  per kill:', {k: round(v, 3) for k, v in agg.items()}, 'avg drops', sim['avg_drops'])
