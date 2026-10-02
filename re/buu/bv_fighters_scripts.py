"""Disassemble the per-spawn event scripts (spawn record +0x14 -> {0x0800424F, script}) of the fighters-group
bosses -> work/buu/fighters_scripts.txt"""
from common import *
from scriptdis import dis_script, script_bytes

RECS = {'Broly 0x0A Volcano Summit': 0x808bd6c, 'Spopovich 0x6D': 0x807a800, 'Spopovich 0x6E': 0x807a828,
        'Spopovich 0x6F': 0x807a850, 'Yamu 0x93': 0x807a878, 'Spopovich 0x70': 0x807a8c8,
        'Yakon 0x91': 0x807d914, 'Dabura 0x12': 0x807f08c, 'Majin Vegeta 0x46 #1': 0x807f140,
        'Majin Vegeta 0x46 #2': 0x807f168, 'Majin Vegeta 0x46 #3': 0x807f190, 'Majin Vegeta 0x46 #4': 0x807f1b8,
        'Vegeta 0x88 Gravity Chamber': 0x8078830, 'Pui Pui 0x60': 0x807cdf4, 'Vegeta 0x89': 0x808d4a8}
out = []
for name, r in RECS.items():
    ex = u32(r + 0x14)
    fn, sc = u32(ex), u32(ex + 4)
    out.append(f'##### {name} rec {r:#x} extra {ex:#x}: fn {fn:#x} script {sc:#x}')
    if fn == 0x0800424F and 0x08000000 <= sc < 0x08800000:
        try:
            data = None
            try:
                data = script_bytes(sc)
            except Exception:  # noqa
                data = None
            code, base = script_bytes(sc, is_idx=False)
            lines = dis_script(base, data=code)
            for a, t in lines:
                out.append(f'    {a:#x} {t}')
        except Exception as e:  # noqa
            out.append(f'    dis failed {e!r}')
(WORK / 'fighters_scripts.txt').write_text('\n'.join(out))
print(len(out))
