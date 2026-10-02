"""Auto-analysis + disassembly of the fighters-group AUTO nodes -> work/buu/fighters_nodes/<fn>.txt
usage: bv_fighters_auto.py [fn ...]   (default: all owned nodes)"""
import json
import sys
import ai2_boot  # noqa
from common import *
from ai_decode import auto
from vtab import entries
from ai_node_dump import full

COMMON = {0x080342a9, 0x080342af, 0x08034307, 0x0804e501, 0x0804f571, 0x0804feb5}
g = json.loads((WORK / 'boss_groups.json').read_text())['fighters']
D = WORK / 'fighters_nodes'
D.mkdir(exist_ok=True)
fns = [int(x, 16) for x in g['auto_nodes']] if len(sys.argv) < 2 else [int(x, 16) for x in sys.argv[1:]]
for fn in fns:
    a = auto(fn)
    out = [json.dumps(a), '--- create', full(fn, 60)]
    vt = int(a['vtable'], 16) if a.get('vtable') else None
    if vt and 0x08000000 <= vt < 0x08800000:
        es = entries(vt, 6)
        out.append('vt entries ' + ' '.join(hex(x) for x in es))
        for i, x in enumerate(es):
            if (i in (0, 1, 2) or (i == 5 and a.get('kind') == 'cond')) and x not in COMMON and 0x08000000 <= x < 0x08800000:
                out.append(f'--- vt[{i}] {hex(x)}')
                out.append(full(x, 200))
    for c in a.get('controllers', []):
        cvt = c.get('vtable')
        if cvt:
            es = entries(int(cvt, 16), 3)
            out.append(f'=== controller {c["controller"]} vt {cvt} ' + ' '.join(hex(x) for x in es))
            for i, x in enumerate(es[:3]):
                if x not in COMMON:
                    out.append(f'--- ctrl[{i}] {hex(x)}')
                    out.append(full(x, 250))
    (D / f'{fn:08x}.txt').write_text('\n'.join(out))
print('ok')
