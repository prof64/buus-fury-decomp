from common import *
from vtab import entries
import sys
for a in map(lambda s: int(s, 0), sys.argv[1:]):
    n = 80
    print(f'--- {a:#x}')
    print(dis(a & ~1, n))
