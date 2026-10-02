"""Disassemble every script in the script table (0x0879B704) to work/buu/scripts_all.txt."""
from common import *
from scriptdis import dis_script, script_bytes, SCRIPTS

out = []
i = 0
while True:
    hdr = u32(SCRIPTS + 4 * i)
    if not (0x08000000 <= hdr < 0x08800000):
        break
    blob = u32(hdr)
    if not (0x08000000 <= blob < 0x08800000):
        out.append(f'=== #{i} hdr {hdr:08x} (bad blob)')
        i += 1
        continue
    comp = u8(blob + 2)
    out.append(f'=== #{i} hdr {hdr:08x} blob {blob:08x} b0={u8(blob)} b1={u8(blob+1)} comp={comp}')
    if comp in (0, 4):
        data, base = script_bytes(i)
        for a, s in dis_script(base, data=data, stop_at_end=False):
            out.append(f'  {a:08x}: {s}')
            if s.startswith('??'):
                break
    i += 1
(WORK / 'scripts_all.txt').write_text('\n'.join(out))
print(i, 'scripts')
