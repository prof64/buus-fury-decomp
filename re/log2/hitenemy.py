from common import *
from ents import enemies
import sys
e=new(sys.argv[1] if len(sys.argv)>1 else 'log2_map12b.ss')
P=e.r32(0x3001fa8)
print('player',hex(P), e.r32(P+0x158)/256, e.r32(P+0x15c)/256)
en=[b for b,c in enemies(e) if e.r32(b+0x188)>0]
T=en[-1]; print('target',hex(T),e.r32(T+0x188))
# put player just left of target facing right
e.w32(P+0x158, e.r32(T+0x158)-24*256); e.w32(P+0x15c, e.r32(T+0x15c))
e.w32(T+0x188, 5000)
e.watch(T+0x188,T+0x18c,'w')
for k in range(6):
    e.press(KEY.RIGHT,1,0)
    e.press(KEY.A,2,20)
seen={}
for h in e.hits():
    k=h['pc']
    seen.setdefault(k,[]).append((h['old'],h['new'],hex(h['lr'])))
for k,v in seen.items(): print(hex(k),v[:5])
print('hp now',e.r32(T+0x188))
e.screenshot(OUT+'hit.png')
