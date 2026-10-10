"""Verify player melee damage formula:
dmg = (ctl_maxHP * (0x180 + rand2(0x80))) >> 9, rand2 = secondary RNG at 0x03003D24 (0x0800A714).
At entry of hit() (0x08007416) called from the punch handler (lr=0x08005695), RNG2 already advanced,
so its output is reconstructible from the post-state: out = (a*b) mod 2^32."""
import random
from game import *

HIT = 0x08007416
e = emu('log1_wolf.ss')
pc = player_char(e)
wolf = [en for en in enemies(e) if en['name'] == 'wolf' and abs(en['x'] - 240) < 50][0]['char']
e.hold(KEY.UP, 20); e.set_keys(0)
random.seed(2)
e.breakpoint(HIT)
ok = bad = 0
for it in range(40):
    M = random.choice([105, 200, 600, 1700, 9000, 150000])
    e.w32(pc + 0x50, M)
    e.w32(wolf + 0x4c, 10 ** 7); e.w32(pc + 0x4c, 10 ** 6)
    for f in range(40):
        st = e.save()
        k = KEY.A if f % 8 < 2 else 0
        e.set_keys(k); e.run(1)
        if not any(h['kind'] == 'break' and h['lr'] == 0x08005695 for h in e.hits()):
            continue
        e.load(st); e.set_keys(k)
        start = e.frame
        while e.frame == start:
            e.step()
            if e.reg(15) == HIT + 4 and e.reg(16) & 0x20 and e.reg(14) == 0x08005695:
                a, b = e.r32(0x03003D24), e.r32(0x03003D28)
                out = (a * b) & 0xFFFFFFFF
                r = ((out >> 16) * 0x80) >> 16
                pred = (M * (0x180 + r)) >> 9 & 0xFFFFFFFF
                got = e.reg(3) & 0xFFFFFFFF
                good = pred == got
                ok += good; bad += not good
                print(M, r, pred, got, good)
        e.hits()
print('ok', ok, 'bad', bad)
