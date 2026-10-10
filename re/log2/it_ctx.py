from common import *
import sys
for a in sys.argv[1:]:
    a=int(a,16)
    print('-----',hex(a))
    print(dis(a-12,12))
