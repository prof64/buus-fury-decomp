"""Print auto-analysis + disassembly of node create fn / vtable fns / controller fns.
usage: bv_buu_nodes.py fn [fn...]  (writes work/buu/bv_buu_nodes_<fn>.txt)"""
import sys
import json
from common import *
from vtab import entries
from ai_decode import auto, emu_ram, REGISTRY


def dis_fn(a, n=120):
    """Disassemble until the next function prologue (push {..lr}) following a return."""
    out = []
    seen_ret = False
    for i, ln in enumerate(dis(a & ~1, max(n, 300)).split('\n')):
        if i > 0 and seen_ret and 'push' in ln and 'lr' in ln:
            break
        out.append(ln)
        if ('pop' in ln and 'pc' in ln) or 'bx lr' in ln:
            seen_ret = True
    return '\n'.join(out)


for a in (sys.argv[1:] if __name__ == '__main__' else []):
    fn = int(a, 16)
    out = [f'=== node {fn:#x}', json.dumps(auto(fn), indent=1), '--- create', dis_fn(fn, 60)]
    au = auto(fn)
    vt = au.get('vtable')
    if vt:
        vt = int(vt, 16)
        if vt < 0x08000000:
            vt = emu_ram().r32(vt)
        if 0x08000000 <= vt < 0x08800000:
            es = entries(vt, 6)
            out.append('vtable ' + ' '.join(hex(x) for x in es))
            for i, x in enumerate(es):
                if i in (0, 1, 2, 5) and 0x08000000 <= x < 0x08100000:
                    out.append(f'--- vt[{i}] {x:#x}')
                    out.append(dis_fn(x, 150))
    for c in au.get('controllers', []):
        cv = c.get('vtable')
        if cv:
            es = entries(int(cv, 16), 3)
            out.append(f'controller {c["controller"]} vt {cv}: ' + ' '.join(hex(x) for x in es))
            for i, x in enumerate(es[:2]):
                out.append(f'--- ctrl vt[{i}] {x:#x}')
                out.append(dis_fn(x, 200))
    (WORK / f'bv_buu_nodes_{fn:08x}.txt').write_text('\n'.join(out))
    print(a, 'ok', len(out))
