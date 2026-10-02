"""script_native_calls.py -- per script native: BL targets (transitively, depth 3), whether it can block
(reaches run_mode 0x0803D8A0), enqueues a command (0x0801936A), and which command vtables it constructs.
Writes work/buu/script/native_calls.json."""
import json
import capstone
from common import *

NAT, SET = 0x08055098, 0x08055478
RUN_MODE = 0x0803D8A0
ENQUEUE = 0x0801936A
COMMIT = 0x0801934E
MODAL_RES = 0x08012588
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
md.detail = False
_cache = {}


def func(a, limit=400):
    """Linear sweep of a Thumb function: returns (bl_targets, literal_words)."""
    a &= ~1
    if a in _cache:
        return _cache[a]
    bls, lits = [], []
    code = rom()[ra(a):ra(a) + limit * 2]
    furthest = a
    for ins in md.disasm(code, a):
        m, op = ins.mnemonic, ins.op_str
        if m == 'bl':
            bls.append(int(op.lstrip('#'), 16))
        elif m == 'ldr' and 'pc' in op:
            off = int(op.split('#')[1].rstrip(']'), 16)
            lit = ((ins.address + 4) & ~3) + off
            lits.append(u32(lit))
        elif m.startswith('b') and m not in ('bl', 'bx', 'blx', 'bic', 'bics') and op.startswith('#'):
            t = int(op.lstrip('#'), 16)
            if t > furthest:
                furthest = t
        if ((m == 'pop' and 'pc' in op) or (m == 'bx')) and ins.address >= furthest:
            break
    _cache[a] = (bls, lits)
    return _cache[a]


def reach(a, depth=3, seen=None):
    seen = seen if seen is not None else set()
    if depth < 0 or a in seen:
        return seen
    seen.add(a)
    if a in (RUN_MODE, ENQUEUE, COMMIT, MODAL_RES):
        return seen
    if not (0x08000000 <= a < 0x08060000):
        return seen
    for t in func(a)[0]:
        reach(t, depth - 1, seen)
    return seen


out = {}
for kind, tab, n in (('native', NAT, 248), ('setter', SET, 18)):
    for i in range(n):
        h = u32(tab + 4 * i)
        bls, lits = func(h)
        r = reach(h & ~1)
        vt = sorted({hex(l) for l in lits if 0x08049000 <= l < 0x0804F000})
        out[f'{kind}_{i:02x}'] = {
            'handler': hex(h), 'bl': [hex(b) for b in bls], 'vtables': vt,
            'run_mode_direct': RUN_MODE in bls,
            'enqueue': ENQUEUE in r, 'commit': COMMIT in r, 'modal_resource': MODAL_RES in r,
            'reaches_run_mode': RUN_MODE in r or ENQUEUE in r or COMMIT in r or MODAL_RES in r,
            'globals': sorted({hex(l) for l in lits if 0x02000000 <= l < 0x04000000}),
        }
(WORK / 'script' / 'native_calls.json').write_text(json.dumps(out, indent=1))
for k, v in out.items():
    flags = [f for f in ('run_mode_direct', 'enqueue', 'commit', 'modal_resource') if v[f]]
    print(k, v['handler'], ' '.join(flags), ' '.join(v['vtables']))
