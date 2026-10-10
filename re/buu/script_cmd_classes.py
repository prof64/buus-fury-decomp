"""script_cmd_classes.py -- dump the per-actor command classes that script natives queue (vtables found by
script_native_calls.py): methods +4 start(cmd, track), +8 update(cmd, track), +0xC done?(cmd, track), with each
method's code until its return. Writes work/buu/script/cmd_classes_asm.txt."""
import json
from common import *
from vtab import entries
from script_native_calls import func

calls = json.loads((WORK / 'script' / 'native_calls.json').read_text())
by_vt = {}
for k, v in calls.items():
    for vt in v['vtables']:
        by_vt.setdefault(vt, []).append(k)
out = []
for vt in sorted(by_vt):
    a = int(vt, 16)
    es = entries(a, 6)
    out.append(f'##### class {vt} used by {by_vt[vt]}  methods ' + ' '.join(f'{x:08x}' for x in es))
    for name, f in zip(('dtor', 'start', 'update', 'done'), es[:4]):
        out.append(f'--- {name} {f:08x}')
        lines = dis(f & ~1, 60).split('\n')
        for l in lines:
            out.append(l)
            if ('pop' in l and 'pc' in l) or 'bx lr' in l:
                break
(WORK / 'script' / 'cmd_classes_asm.txt').write_text('\n'.join(out) + '\n')
print(len(by_vt), 'classes')
