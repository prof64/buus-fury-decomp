from common import *
import sys
base=int(sys.argv[1],16); n=int(sys.argv[2]) if len(sys.argv)>2 else 40
for k in range(n):
    w=u32(base+4*k); t=(base+w)&0xffffffff
    print(k, hex(k*4), hex(t))
