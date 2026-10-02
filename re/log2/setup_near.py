# exec'd by bp.py: put player left of last live enemy, set enemy hp high
from ents import enemies
P=e.r32(0x3001fa8)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]
T=en[-1]
e.w32(P+0x158, e.r32(T+0x158)-24*256); e.w32(P+0x15c, e.r32(T+0x15c))
e.w32(T+0x188, 5000)
print('player',hex(P),'target',hex(T))
