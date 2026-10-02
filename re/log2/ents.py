from common import *
import sys
CT=0x86d56ac
def enemies(e):
    d=e.ewram(); out=[]
    for i in range(0,len(d)-4,4):
        v=struct.unpack_from('<I',d,i)[0]
        if CT<=v<CT+109*24 and (v-CT)%24==0:
            base=0x02000000+i-0x180
            vt=e.r32(base)
            if vt==0x80252a4 or True:
                out.append((base,(v-CT)//24))
    return out
if __name__=='__main__':
    e=new(sys.argv[1])
    for b,c in enemies(e):
        print(hex(b),'class',c,'vt',hex(e.r32(b)),'hp',e.r32(b+0x188),'x',e.r32(b+0x158)/256,'y',e.r32(b+0x15c)/256)
