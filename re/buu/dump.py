"""dump.py state addr len  -- hexdump RAM from a state"""
import sys
from common import *
st = sys.argv[1]
e = emu(st if st.endswith('.ss') and '/' not in st and (STATES / st).exists() else (WORK / st))
a = int(sys.argv[2], 16); n = int(sys.argv[3], 16) if len(sys.argv) > 3 else 0x100
b = e.read(a, n)
for i in range(0, n, 16):
    row = b[i:i + 16]
    words = ' '.join(f'{int.from_bytes(row[j:j+4],"little"):08x}' for j in range(0, len(row), 4))
    print(f'{a+i:08x}: {words}')
