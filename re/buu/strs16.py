"""Dump UTF-16LE strings: strs16.py [minlen] [regex]"""
import re, sys
from common import rom
minlen = int(sys.argv[1]) if len(sys.argv) > 1 else 4
pat = re.compile(sys.argv[2]) if len(sys.argv) > 2 else None
r = rom()
for m in re.finditer(rb'(?:[\x20-\x7e\n]\x00){%d,}' % minlen, r):
    if m.start() & 1:
        continue
    s = m.group().decode('utf-16le')
    if pat and not pat.search(s):
        continue
    print(f'{0x08000000 + m.start():08x} {s!r}')
