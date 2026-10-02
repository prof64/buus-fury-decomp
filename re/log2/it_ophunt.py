from common import *
T=0x83b5c00
seen=set()
for op in range(0x200):
    h=u32(T+4*op)
    if h>>24!=8 or h in seen: continue
    seen.add(h)
    d=dis(h&~1,60)
    body=d.split('pop')[0] if 'pop' in d else d
    if ('cmp r' in body and '#6' in body) and ('#2]' in body or '#6]' in body):
        print(hex(op),hex(h))
