"""disfuncs.py addr [addr...] [-n N] -- disassemble several functions (until first pop {...pc} / bx lr)"""
import sys
from common import *
args = sys.argv[1:]
n = 60
STOP = '-all' not in args
args = [x for x in args if x != '-all']
if '-n' in args:
    i = args.index('-n'); n = int(args[i + 1]); del args[i:i + 2]
for s in args:
    a = int(s, 16) & ~1
    print(f'=== {a:08x}')
    lines = dis(a, n).split('\n')
    for l in lines:
        print(l)
        if (('pop' in l and 'pc' in l) or 'bx lr' in l) and STOP:
            break
