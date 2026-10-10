"""pdis.py START END : print lines of work/log1_dis.txt whose address is in [START, END)."""
import sys
from common import ROOT
s, e = int(sys.argv[1], 16), int(sys.argv[2], 16)
for line in open(ROOT / 'work' / 'log1_dis.txt'):
    if line[:8].strip() and line[8:9] == ':':
        try:
            a = int(line[:8], 16)
        except ValueError:
            continue
        if s <= a < e:
            print(line.rstrip())
    elif line.startswith(';----'):
        try:
            a = int(line.split()[-1], 16)
            if s <= a < e:
                print(line.rstrip())
        except ValueError:
            pass
