"""romdump.py addr len [w|b|h]"""
import sys
from common import *
a = int(sys.argv[1], 16); n = int(sys.argv[2], 16)
mode = sys.argv[3] if len(sys.argv) > 3 else 'w'
r = rom()
for i in range(0, n, 16):
    row = r[ra(a) + i: ra(a) + i + 16]
    if mode == 'w':
        s = ' '.join(f'{int.from_bytes(row[j:j+4],"little"):08x}' for j in range(0, 16, 4))
    elif mode == 'h':
        s = ' '.join(f'{int.from_bytes(row[j:j+2],"little"):04x}' for j in range(0, 16, 2))
    else:
        s = row.hex(' ')
    print(f'{a+i:08x}: {s}')
