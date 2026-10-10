from common import *
from collections import Counter
e=Emu('log2')
e.watch(0x03003890,0x03003898,'w')
e.run(1)
print('f1 state',hex(e.r32(0x3003890)),hex(e.r32(0x3003894)))
c=Counter()
for f in range(900):
    e.run(1)
    for h in e.hits(): c[(h['pc'],h['lr'])]+=1
    if f in (10,300,899): print('f',f,hex(e.r32(0x3003890)),hex(e.r32(0x3003894)))
for k,v in c.items(): print(hex(k[0]),hex(k[1]),v)
