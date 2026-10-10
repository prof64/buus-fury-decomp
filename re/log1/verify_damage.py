"""Verify the damage/defense function model (model.apply_damage) against the emulator.
Wolf vs Goku fight on map 2; defense values of both are randomised every 100 frames,
HP kept high. At each entry of 0x08007352 we snapshot RNG + args, and compare the defender's
HP after the call (breakpoint at the function's return sites is unnecessary: we compare
HP one instruction-stream later using step())."""
import random
from game import *
from model import Rng, apply_damage

DMG = 0x08007352
e = emu('log1_wolf.ss')
pc = player_char(e)
wolf = [en for en in enemies(e) if en['name'] == 'wolf' and abs(en['x'] - 240) < 50][0]['char']
random.seed(1)
e.hold(KEY.UP, 20); e.set_keys(0)
bp = e.breakpoint(DMG)
ok = bad = 0
results = []
for it in range(60):
    for c in (pc, wolf):
        e.w32(c + 0x60, random.choice([0, 1, 10, 30, 50, 100, 200, 250, 400]))
        e.w32(c + 0x4c, 3000)
    e.w32(0x03000C18 + 4, 1000)   # enemy damage cap (normally maxHP/3+1)
    for f in range(100):
        st = e.save()
        e.set_keys(KEY.A if f % 8 < 2 else 0)
        e.run(1)
        hs = [h for h in e.hits() if h['kind'] == 'break']
        if not hs:
            continue
        # replay this frame instruction-by-instruction to capture exact state at entry
        e.load(st)
        e.set_keys(KEY.A if f % 8 < 2 else 0)
        start = e.frame
        while e.frame == start:
            e.step()
            if e.reg(15) == DMG + 4 and e.reg(16) & 0x20:
                att, dfn, dmg = e.reg(0), e.reg(1), e.reg(3)
                rng = Rng(e.r32(0x0300098C), e.r32(0x03000990))
                hp0 = s32v(e.r32(dfn + 0x4c)); dfe = s32v(e.r32(dfn + 0x60))
                fl = (e.r16(dfn + 0x9a), e.r8(dfn + 0xa), e.r8(dfn + 0xb), e.r32(0x03000C18))
                lr = e.reg(14)
                # run until return to caller
                while not (e.reg(15) in (lr + 4 - 1, lr - 1 + 4) and e.reg(16) & 0x20):
                    e.step()
                    if (e.reg(15) - 4) == (lr & ~1):
                        break
                res = e.reg(0)
                hp1 = s32v(e.r32(dfn + 0x4c))
                pres, php = apply_damage(rng, dmg, dfe, hp0, *fl)
                good = (pres == res and php == hp1)
                ok += good; bad += (not good)
                results.append((hex(att), hex(dfn), dmg, dfe, hp0, hp1, res, pres, php, fl, good))
        e.hits()
for r in results:
    print(r)
print('ok', ok, 'bad', bad)
