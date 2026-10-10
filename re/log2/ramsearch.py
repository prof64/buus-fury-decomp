from common import *
import sys
st=sys.argv[1]; lo=int(sys.argv[2],16); hi=int(sys.argv[3],16)
e=new(st)
for base,data in ((0x02000000,e.ewram()),(0x03000000,e.iwram())):
    for i in range(0,len(data)-3,4):
        v=struct.unpack_from('<I',data,i)[0]
        if lo<=v<hi: print(hex(base+i),hex(v))
