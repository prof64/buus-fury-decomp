"""Check diagonal input handling (one axis per frame?)."""
from game import *

e = emu('log1_wolf.ss')
for combo in (('DOWN', 'RIGHT'), ('LEFT', 'DOWN'), ('RIGHT', 'UP'), ('LEFT', 'RIGHT')):
    a = player_xy(e)
    kk = 0
    for n in combo:
        kk |= getattr(KEY, n)
    e.hold(kk, 10)
    e.set_keys(0)
    e.run(2)
    print(combo, a, player_xy(e))
