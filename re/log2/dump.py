from common import *
import sys
st=sys.argv[1]; a=int(sys.argv[2],16); n=int(sys.argv[3],16)
e=new(st)
if len(sys.argv)>4: e.run(int(sys.argv[4]))
b=e.read(a,n)
for i in range(0,n,16):
    row=b[i:i+16]
    print(f'{a+i:08x}: '+' '.join(f'{struct.unpack_from("<H",row,j)[0]:04x}' for j in range(0,16,2)))
