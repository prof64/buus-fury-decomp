# exec'd by verify_dmg.py: place player next to last live enemy, randomize stats on both sides
import random, os
from ents import enemies
random.seed(int(os.environ.get('SEED','1')))
P=e.r32(0x3001fa8)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]
T=en[-1]
e.w32(P+0x158, e.r32(T+0x158)-24*256); e.w32(P+0x15c, e.r32(T+0x15c))
e.w32(T+0x188, 900000)
for k,v in enumerate([random.randint(1,100),random.randint(1,100),random.randint(0,127),random.choice([0x20,0x40,0x80,0xc0]),random.choice([0x20,0x40,0x80,0xc0])]):
    e.w8(P+0x160+k,v)
for k,v in enumerate([random.randint(1,100),random.randint(1,100),random.randint(0,127),random.choice([0x26,0x40,0x80,0xff]),random.choice([0x26,0x40,0x80,0xff])]):
    e.w8(T+0x160+k,v)
print('player stats',e.read(P+0x160,5).hex(),'enemy stats',e.read(T+0x160,5).hex())
