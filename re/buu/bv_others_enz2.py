"""Enzyme: count calls of the latch cond (0x08037B48), drain update (0x08029902), drain apply (0x08010BF2),
mash cond (0x080363C4) per frame window; player starts overlapping, not pinned."""
from common import *
from bv_buu_lib import setup, pos, REC
from ents import PLAYER

e, o = setup(0x18, dx=-30, dy=0)
p = e.r32(PLAYER)
names = {0x08037B48: 'latchcond', 0x08029902: 'drainupd', 0x08010BF2: 'hploss', 0x080363C4: 'mashcond',
         0x080298D8: 'drainenter', 0x0800AA4A: 'goto'}
ids = [e.breakpoint(a) for a in names]
cnt = {}
first = {}
for f in range(600):
    e.set_keys(KEY.RIGHT if f < 30 else (KEY.A if (400 <= f < 460 and f % 4 == 0) else 0))
    e.run(1)
    for h in e.hits():
        n = names.get(h['addr'])
        if n == 'goto' and h['regs'][0] != o + 0x238:
            continue
        cnt[n] = cnt.get(n, 0) + 1
        first.setdefault(n, (f, [hex(x) for x in h['regs'][:3]]))
        if n in ('hploss', 'goto', 'drainenter'):
            print(f, n, [hex(x) for x in h['regs'][:3]], 'HP', e.r16(REC + 0x16))
print(cnt)
print(first)
print('player', pos(e, p), 'enzyme', pos(e, o), 'HP', e.r16(REC + 0x16))
