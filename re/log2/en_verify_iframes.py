"""Verify enemy hit reaction: after a player melee hit in open ground, an ordinary enemy (snake, on-hit via state 3)
pushes state 1 (KnockbackInvulnerable(40)) -> entity+0x168 (hittable) = 0 until the hurt anim ends.
Logs per frame (from the frame the HP drops): AI stack, hittable flag, HP, position."""
from common import *
from en_verify_aggro import stack, place, center
e=new('log2_map12b.ss')
P=e.r32(0x3001fa8); T=0x02037be4
e.w32(T+0x188,5000)
rows=[]; hit=None
for f in range(400):
    if hit is None:
        place(e,P,T,-24,0)
        e.set_keys(KEY.A if f%20<2 else (KEY.RIGHT if f%20==3 else 0))
    else:
        e.set_keys(0)
    e.run(1)
    if hit is None and e.r32(T+0x188)<5000: hit=f
    if hit is not None:
        rows.append((f-hit,stack(e,T),e.r32(T+0x168),e.r32(T+0x188),tuple(round(v,1) for v in center(e,T))))
        if f-hit>90: break
prev=None
for r in rows:
    key=(tuple(r[1]),r[2])
    if key!=prev: print(r); prev=key
print('last',rows[-1] if rows else None)
