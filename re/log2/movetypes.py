"""Decode the special-move type table at 0x081d9900 (14 x 16 bytes): [icon0, icon1, canUse(fn), execute(fn)].
canUse: requires global attack-enable flag and record.EP >= cost; execute: EP -= cost, player.setState(state)."""
from common import *
import re
TBL=0x081d9900
def scan(fn, n=40):
    if not fn: return {}
    out={}
    for line in dis(fn&~1, n).split('\n'):
        m=re.search(r'cmp r1, #(0x[0-9a-f]+|\d+)',line)
        if m and 'cost_cmp' not in out: out['cost_cmp']=int(m.group(1),0)
        m=re.search(r'subs r2, #(0x[0-9a-f]+|\d+)',line)
        if m and 'cost_sub' not in out: out['cost_sub']=int(m.group(1),0)
        m=re.search(r'movs r1, #(0x[0-9a-f]+|\d+)',line)
        if m and 'state' not in out and 'cost_sub' in out: out['state']=int(m.group(1),0)
        if ('pop' in line) or ('bx lr' in line and 'cost_cmp' in out): break
    return out
def table():
    rows=[]
    for i in range(14):
        ic0,ic1,can,ex=struct.unpack_from('<4I',ROM,(TBL&0x1ffffff)+16*i)
        a=scan(can); b=scan(ex)
        rows.append(dict(id=i,icon0=hex(ic0),icon1=hex(ic1),can_use_fn=hex(can),execute_fn=hex(ex),
                         ep_cost=b.get('cost_sub',a.get('cost_cmp')),min_ep=a.get('cost_cmp'),player_state=b.get('state')))
    return rows
if __name__=='__main__':
    for r in table(): print(r)
