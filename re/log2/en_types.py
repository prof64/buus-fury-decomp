from common import *
import re
types="801b63f 801ab65 801aef3 801ba47 801b887 801c05b 801b861 801d749 801b511 801bbad 801b333 801a71d 801a947 801ea59 801be8b 801b15f 801c95f 801acb7 801c2ad 801c431 801e77b 801b4f3 801d255 801c7f3 801c451 801a9a5 801a825 801a98d 801bb89 801c085 801c0d7 801bc6f 801e5c1 801dbb3 801bd17 801bca9 801bcf1 801c85b 801e145 801bb6d 801dcdb 801c82d 801cf73 801cca9 801a7df 801ca85 801ca43 801c5a3 801b991".split()
for t in types:
    a=int(t,16)&~1
    txt=dis(a,40)
    vts=[]
    for line in txt.split('\n'):
        m=re.search(r'=0x(80[23][0-9a-f]{4})$',line)
        if m: vts.append(int(m.group(1),16))
        if 'pop' in line: break
    out=[]
    for v in vts:
        if 0x8023000<=v<0x8026000:
            out.append(hex(v)+':'+','.join(hex((v+u32(v+4*k))&0xffffffff) for k in range(4)))
    print(t, out)
