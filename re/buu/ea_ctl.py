"""ea_ctl.py vt [vt...] -- print an actor state object's vtable (enter 0, tick 1, control 2, end 9, 10) and disassemble
enter / tick / end (stops at the first return)."""
import sys
from common import *
from vtab import entries

GENERIC = {0x0800DE6C: 'once', 0x0800DEB0: 'loop', 0x0800DFD4: 'idle', 0x0800DF8A: 'bored', 0x0804E4E0: 'hold',
           0x0800DE60: 'enter0', 0x0800DE94: 'end->SetAnim(0)', 0x0800DED0: 'end->hold', 0x0800DEA6: 'end:none',
           0x0800DEE8: 'enter_punch(sfx 50+rand2)', 0x0800DFBE: 'enter_idle(tick=rand255)', 0x0800DEDC: 'enter0b',
           0x0804FEAC: 'ctl_default', 0x0800DE3C: 'nop'}


def fn_text(f, n=200, full=False):
    """disassemble until a return that no earlier forward branch jumps past"""
    import re
    lines = dis(f & ~1, n).split('\n')
    out = []
    far = 0
    for l in lines:
        out.append('    ' + l)
        a = int(l.split(':')[0], 16)
        m = re.search(r'\s(b\w*)\s+#0x([0-9a-f]+)$', l)
        if m and m.group(1) not in ('bl', 'blx'):
            far = max(far, int(m.group(2), 16))
        if (('pop' in l and 'pc' in l) or 'bx lr' in l or re.search(r'\sb\s+#', l)) and a >= far:
            break
    return '\n'.join(out)


for s in sys.argv[1:]:
    vt = int(s, 16)
    ents = entries(vt, 11)
    print('=== vt %08x' % vt, ' '.join('%d:%08x%s' % (i, f, '(' + GENERIC[f & ~1] + ')' if (f & ~1) in GENERIC else '')
                                     for i, f in enumerate(ents)))
    for slot in (0, 1, 9):
        f = ents[slot] & ~1
        if f in GENERIC:
            continue
        print('  -- slot %d %08x' % (slot, f))
        print(fn_text(f))
