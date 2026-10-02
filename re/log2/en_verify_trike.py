"""Verify class 94 Triceratops (ai_state 15 = Patrol + ContactDamage, on-hit = GlowFlash only): melee/ki hits
should not reduce its HP. Warp to map 103 (zone 4 area 56, Triceratops Jungle), find a class-94 entity, attack it."""
from common import *
from warp import warp
from ents import enemies
from en_verify_aggro import stack, place
e=warp(103,0,300)
for k in range(20): e.press(KEY.B,2,10)   # clear any dialog
P=e.r32(0x3001fa8)
T=[b for b,c in enemies(e) if c==94]
print('class94 entities',len(T))
if T:
    T=T[0]; hp=e.r32(T+0x188); print('hp',hp,'stack',stack(e,T))
    for btn,name in ((KEY.A,'melee'),(KEY.B,'ki')):
        d=[]
        for k in range(8):
            place(e,P,T,-24,0); e.press(KEY.RIGHT,1,0)
            h0=e.r32(T+0x188); e.press(btn,2,20); d.append(e.r32(T+0x188)-h0)
        print(name,'hp deltas',d,'player hp',e.r16(0x3000e9c+28*e.r8(0x3000e90)))
