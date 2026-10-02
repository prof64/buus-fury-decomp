from common import *
import sys
T=0x086d5194
lo=int(sys.argv[1]) if len(sys.argv)>1 else 0
hi=int(sys.argv[2]) if len(sys.argv)>2 else 152
for i in range(lo,hi):
    p0,p1=struct.unpack_from('<2I',ROM,(T&0x1ffffff)+8*i)
    w0=[u32(p0+4*k) for k in range(8)]
    w1=[u32(p1+4*k) for k in range(4)]
    print(i,hex(p0),' '.join(hex(x) for x in w0),'|',hex(p1),' '.join(hex(x) for x in w1))
