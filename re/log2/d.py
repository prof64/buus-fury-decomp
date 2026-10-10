from common import *
import sys
a=int(sys.argv[1],16); n=int(sys.argv[2]) if len(sys.argv)>2 else 60
print(dis(a,n,arm=len(sys.argv)>3))
