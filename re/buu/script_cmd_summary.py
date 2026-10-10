"""script_cmd_summary.py -- one line per command class: natives, start/update/done methods, the class of the
done method (instant / frame counter / flag / other) and the BL targets of start/update (for naming)."""
import json
from common import *
from vtab import entries
from script_native_calls import func

calls = json.loads((WORK / 'script' / 'native_calls.json').read_text())
by_vt = {}
for k, v in calls.items():
    for vt in v['vtables']:
        by_vt.setdefault(vt, []).append(k.replace('native_', ''))
KNOWN = {0x0804fb95: 'instant(1)', 0x080159f5: 'count(+8)', 0x0804febd: 'nop', 0x0804ffdd: 'nop',
         0x0804e549: 'nop', 0x0804e4fd: 'nop', 0x0804f55d: 'delete-arg', 0x0800700d: 'mode-frame'}
rows = []
for vt in sorted(by_vt, key=lambda v: by_vt[v]):
    es = entries(int(vt, 16), 4)
    st, up, dn = es[1], es[2], es[3]
    d = KNOWN.get(dn & ~0 if dn in KNOWN else dn)
    if d is None:
        first = dis(dn & ~1, 3).replace('\n', ' | ')
        d = first[:90]
    bl_s = [hex(b) for b in func(st)[0]][:5]
    bl_u = [hex(b) for b in func(up)[0]][:5]
    rows.append(f'{vt} nat {",".join(by_vt[vt])}: start {st:08x} {bl_s} upd {up:08x} '
                f'{KNOWN.get(up, "")} {bl_u} done {dn:08x} {d}')
(WORK / 'script' / 'cmd_summary.txt').write_text('\n'.join(rows) + '\n')
print('\n'.join(rows))
