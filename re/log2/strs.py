from common import *
import sys, re
a=int(sys.argv[1],16); b=int(sys.argv[2],16); mn=int(sys.argv[3]) if len(sys.argv)>3 else 3
for m in re.finditer(rb'[\x20-\x7e\n]{%d,}'%mn, ROM[a:b]):
    print(hex(0x08000000+a+m.start()), m.group().decode()[:200].replace('\n','\\n'))
