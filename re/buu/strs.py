"""Dump printable ASCII strings with offsets: strs.py [minlen] [regex]"""
import re, sys
from common import rom
minlen = int(sys.argv[1]) if len(sys.argv) > 1 else 6
pat = re.compile(sys.argv[2]) if len(sys.argv) > 2 else None
r = rom()
for m in re.finditer(rb'[\x20-\x7e\n]{%d,}' % minlen, r):
    s = m.group().decode()
    if pat and not pat.search(s):
        continue
    print(f'{0x08000000 + m.start():08x} {s!r}')
