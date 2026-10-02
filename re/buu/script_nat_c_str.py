"""script_nat_c_str.py id... -- print string table entries."""
import sys
from strtab import ustr, STRTAB
from common import u32

for s in sys.argv[1:]:
    i = int(s, 0)
    print(i, repr(ustr(u32(STRTAB + 4 * i))[:120]))
