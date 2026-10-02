"""log_boss.py STATE FRAMES : log every hit()/ki-blast spawn for a boss fight with the player kept alive
(player HP set high, player alternately walks to provoke decisions)."""
import sys
from collections import Counter
from game import *

e = emu(sys.argv[1])
frames = int(sys.argv[2])
pc = player_char(e)
e.w32(pc + 0x50, 10 ** 6); e.w32(pc + 0x4c, 10 ** 6)
e.breakpoint(0x08007416); e.breakpoint(0x0800C010)
cnt = Counter()
for f in range(frames):
    e.set_keys([KEY.LEFT, 0, KEY.RIGHT, 0][(f // 30) % 4])
    e.run(1)
    for h in e.hits():
        r = h['regs']; p = h['pc'] - 2
        if p == 0x0800C010:
            o = e.r32(r[0] + 0x3c)
            cnt['ki_spawn'] += 1
            print(e.frame, 'ki blast spawn by', template_name(e.r16(o)), 'power', r[2])
        elif p == 0x08007416:
            kind = 'ki' if r[2] else 'melee'
            cnt[(template_name(e.r16(r[0])) if r[0] != pc else 'goku', kind)] += 1
            print(e.frame, 'hit', 'goku' if r[0] == pc else template_name(e.r16(r[0])), '->',
                  'goku' if r[1] == pc else template_name(e.r16(r[1])), kind, 'raw', s32v(r[3]))
print(cnt)
for en in enemies(e):
    if en['hp'] > 1000:
        print(en['name'], en['hp'], en['x'], en['y'])
