"""scriptscan.py regex -- disassemble all (uncompressed) scripts in the script table and grep."""
import re
import sys
from common import *
from scriptdis import dis_script, SCRIPTS

pat = re.compile(sys.argv[1])
n = 0
i = 0
while True:
    hdr = u32(SCRIPTS + 4 * i)
    if not (0x08000000 <= hdr < 0x08800000):
        break
    blob = u32(hdr)
    if not (0x08000000 <= blob < 0x08800000):
        i += 1
        continue
    comp = u8(blob + 2)
    if comp == 0:
        lines = dis_script(blob + 3, 20000, stop_at_end=False)
        txt = '\n'.join(f'{a:08x}: {s}' for a, s in lines)
        if pat.search(txt):
            print(f'=== script #{i} blob {blob:08x}')
            for a, s in lines:
                if pat.search(s):
                    print(f'   {a:08x}: {s}')
    i += 1
print('scripts', i)
