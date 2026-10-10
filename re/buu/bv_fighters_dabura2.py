"""Dabura spit barrage: force state 0x7c (aim, wait 42, spit; x10) with the player 100 px left; log projectile
tracks, hits and the player's animation pointer (petrify), then what Dabura does while the player is stone."""
from bv_fighters_lib import *

p50 = lambda e, o: hex(e.r32(e.r32(PLAYER) + 0x50))
cnt = lambda e, o: e.r32(o + 0x1e4)
e, o, L = fight(0x12, 900, dist=100, state=0x7c, watch=(p50, cnt))
print(summarize(L, o))
for w in L.watch:
    print('watch', w)
