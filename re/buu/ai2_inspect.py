"""ai2_inspect.py fn [fn...] -- print a behaviour node type: create fn, runtime vtable (allocated: literal in
create; singleton: IWRAM object read from a save state) and its enter/update/exit/condition functions."""
import ai2_boot  # noqa: F401
import sys
import capstone
from common import *
from vtab import entries

md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)
COMMON = {0x080342a9: 'COND-enter', 0x080342af: 'COND-update(child while vt5)', 0x08034307: 'COND-exit',
          0x0804e501: 'nop', 0x0804f571: 'nop', 0x0804feb5: 'ret0', 0x0804f3bd: '?', 0x0803429f: 'ret child.f?'}
_e = None


def emu_state():
    global _e
    if _e is None:
        _e = emu('buu_fight_swforest.ss')
    return _e


def lits(fn, n=40):
    out = []
    code = rom()[ra(fn & ~1):ra(fn & ~1) + n * 2]
    for ins in md.disasm(code, fn & ~1):
        if ins.mnemonic == 'ldr' and 'pc' in ins.op_str:
            off = int(ins.op_str.split('#')[1].rstrip(']'), 16)
            out.append(u32(((ins.address + 4) & ~3) + off))
        if ins.mnemonic == 'pop' and 'pc' in ins.op_str or ins.mnemonic == 'bx':
            break
    return out


def singleton_add(fn):
    """`ldr r0,=IWRAM; adds r0,#n; bx lr` creates return an object embedded at IWRAM+n."""
    code = rom()[ra(fn & ~1):ra(fn & ~1) + 8]
    for ins in md.disasm(code, fn & ~1):
        if ins.mnemonic == 'adds' and ins.op_str.startswith('r0, #'):
            return int(ins.op_str.split('#')[1], 16)
        if ins.mnemonic == 'bx':
            break
    return 0


def runtime_vtable(fn):
    ls = lits(fn)
    vts = [x for x in ls if 0x08049000 <= x < 0x0804f000]
    if vts:
        return vts[-1], 'alloc'
    io = [x for x in ls if 0x03000000 <= x < 0x03008000]
    if io:
        a = io[0] + singleton_add(fn)
        vt = emu_state().r32(a)
        return vt, f'singleton@{a:08x}'
    return None, None


def full(f, n=120):
    out = []
    seen_ret = False
    for i, l in enumerate(dis(f & ~1, n).split('\n')):
        if i > 0 and seen_ret and 'push' in l and 'lr' in l:
            break
        out.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l:
            seen_ret = True
    return '\n'.join(out)


if __name__ == '__main__':
    slots = (0, 1, 2, 5)
    for s in sys.argv[1:]:
        f = int(s, 16)
        print(f'######## {f:08x}')
        print(full(f, 30))
        vt, how = runtime_vtable(f)
        print('vtable', vt and hex(vt), how)
        if vt and 0x08000000 <= vt < 0x08800000:
            ents = entries(vt, 8)
            for sl in slots:
                fn = ents[sl]
                if fn in COMMON:
                    print(f'-- vt[{sl}] {fn:08x} {COMMON[fn]}')
                else:
                    print(f'-- vt[{sl}] {fn:08x}')
                    print(full(fn))
