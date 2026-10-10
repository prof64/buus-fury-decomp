"""Find cutscene steps (cmd 0 VM scripts) anywhere in the map-data region that call spawnMapObject (builtin 0x54)
or replaceActorWithMapObject (0x68); report the step, its script, and the enclosing map (by data range)."""
from en_lib import *
from bs_vm import disasm, cutscene
import json
maps=json.load(open(ROOT+'data/log2/maps.json'))['maps']
# map data ranges: approximate by objdef addresses
rng=[]
for m in maps:
    addrs=[int(o['addr'],16) for o in m['objects']]+[int(x,16) for x in m['raw'] if x.startswith('0x83') or x.startswith('0x84')]
    if addrs: rng.append((min(addrs),max(addrs),m['index'],m['name'],m['zone'],m['area']))
def owner(a):
    best=None
    for lo,hi,i,n,z,ar in rng:
        if lo-0x1000<=a<=hi+0x1000:
            d=0 if lo<=a<=hi else min(abs(a-lo),abs(a-hi))
            if best is None or d<best[0]: best=(d,i,n,z,ar)
    return best
lo=0x003b0000; hi=0x00410000
hits=[]
for o in range(lo,hi-4):
    b=ROM[o]
    # step header: u16 next, u8 cmd==0 at o, script starts o+3
    if ROM[o+2]!=0: continue
    nxt=ROM[o]|ROM[o+1]<<8
    if nxt>200: continue
    seg=ROM[o+3:o+3+200]
    if b'\x11' not in seg: continue
    end=seg.index(b'\x11')
    s=seg[:end+1]
    if not ((b'\x02\x54' in s) or (b'\x9e' in s) or (b'\x02\x68' in s) or (b'\xb2' in s)): continue
    try: dis=disasm(0x08000000+o+3)
    except Exception: continue
    txt=' '.join(dis)
    if 'spawnMapObject' in txt or 'replaceActor' in txt:
        if '?' in txt: continue
        hits.append((0x08000000+o,txt))
seen=set()
for a,t in hits:
    if t in seen and False: continue
    seen.add(t)
    print(hex(a), owner(a), t)
