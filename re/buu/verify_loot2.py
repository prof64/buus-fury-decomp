"""Exact loot check: snapshot the RNG when the game starts the loot script, replay it with the Python VM
using the game's RNG algorithm, and compare the drop sequences."""
import random
import sys
from collections import Counter
from common import *
from emutools import catch
from ents import enemies
from vm import VM
from rng import Rng

NAT = {0x0800253C: 'zenie', 0x08002556: 'supply', 0x080025A4: 'equip'}
RUNNER = 0x08004098
REC = 0x03001560
H497 = u32(0x0879B704 + 4 * 497)


class GameRngVM(VM):
    def __init__(self, rng):
        super().__init__()
        self.grng = rng

    def rand16(self):
        return self.grng.next() & 0xFFFF

    # Spawning a pickup object consumes RNG for its scatter motion (traced in t_loot4.py:
    # coin = 1 call from 0x08017FCD + 4 from rand(); equip = 4 (fits 60/60; 1,5,6 do not), supply = 5 assumed like coins).
    SPAWN_COST = {0x84: 5, 0x85: 5, 0x86: int(sys.argv[1]) if len(sys.argv) > 1 else 4}

    def native(self, n, st):
        super().native(n, st)
        for _ in range(self.SPAWN_COST.get(n, 0)):
            self.grng.next()


rnd = random.Random(11)
base = emu('buu_fight_swforest.ss')
st0 = base.save()
match = 0
total = 0
for trial in range(60):
    e = base
    e.load(st0)
    e.w32(0x03004a5c, rnd.getrandbits(32)); e.w32(0x03004a60, rnd.getrandbits(32))
    en = enemies(e)
    o = en[0]
    p = e.r32(0x03001c34)
    e.w32(o + 0x28c, 1 << 10)
    e.w32(REC + 0x24, 60000000)
    e.w8(p + 0x10, 3)
    e.w32(o + 0x1a0, e.r32(p + 0x1a0) + (18 << 8)); e.w32(o + 0x1a4, e.r32(p + 0x1a4))
    game = []
    snap = {}

    def cb(e, pc):
        if pc == RUNNER:
            if e.reg(1) == H497 and 'rng' not in snap:
                pb = e.reg(2)
                snap['rng'] = (e.r32(0x03004a5c), e.r32(0x03004a60))
                snap['P'] = [e.r32(pb + 4 * k) - (1 << 32 if e.r32(pb + 4 * k) & 0x80000000 else 0) for k in range(4)]
                snap['G'] = [e.r32(0x03001058 + 4 * k) for k in range(4)]
        else:
            # argument is on the VM stack: read top of the script stack (r0 = ctx; top = [ctx + 4*count])
            ctx = e.reg(0)
            cnt = e.r32(ctx)
            game.append((NAT[pc], e.r32(ctx + 4 * cnt)))

    catch(e, [RUNNER] + list(NAT), 200, lambda f: KEY.A if f < 2 else 0, cb)
    if 'rng' not in snap:
        continue
    vm = GameRngVM(Rng(*snap['rng']))
    vm.P = list(snap['P']); vm.G = list(snap['G'])
    vm.run(497)
    sim = [(k, v) for k, v in vm.drops]
    # zenie natives receive the coin size; VM records the value
    sim_sizes = [(k, [5, 25, 50, 100].index(v) if k == 'zenie' else v) for k, v in sim]
    total += 1
    ok = sim_sizes == game
    match += ok
    if not ok and total - match <= 5:
        print('MISMATCH P', snap['P'], 'G', snap['G'])
        print('  game', game)
        print('  vm  ', sim_sizes)
print(f'{match}/{total} kills reproduced exactly')
