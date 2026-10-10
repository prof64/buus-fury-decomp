"""bv_regular_node.py create_fn [...] -- dump a node type: example definitions (raw words, from
enemy_ai.json trees), create fn, runtime vtable slots 0/1/2/5, and any motion controllers it switches to.
Singleton nodes (create = ldr r0,=X; [adds/subs r0,#k]; bx lr) are resolved exactly."""
import json
import sys
import capstone
import ai2_boot  # noqa: F401
from common import *
from vtab import entries
COMMON = {0x080342a9: 'cond-decorator enter', 0x080342af: 'cond-decorator update (runs child while vt[5] true)',
          0x08034307: 'cond-decorator exit', 0x0804e501: 'nop', 0x0804f571: 'nop', 0x0804feb5: 'return 0'}


def full(f, n=90):
    out = []
    seen_ret = False
    for i, l in enumerate(dis(f & ~1, n).split('\n')):
        if i > 0 and seen_ret and 'push' in l and 'lr' in l:
            break
        out.append(l)
        if ('pop' in l and 'pc' in l) or 'bx lr' in l:
            seen_ret = True
    return '\n'.join(out)
from ai_decode import runtime_vtable, emu_ram, fn_features

d = json.loads((DATA / 'enemy_ai.json').read_text())
md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)


def defs_of(fn):
    out = []

    def walk(n, st):
        if int(n['fn'], 16) == fn:
            out.append((st, n['addr']))
        for c in n.get('children', []):
            walk(c, st)
    for st, s in d['states'].items():
        walk(s['tree'], st)
    return out


def node_vt(fn):
    a = fn & ~1
    ins = list(md.disasm(rom()[a - 0x08000000:a - 0x08000000 + 16], a))
    if ins and ins[0].mnemonic == 'ldr' and 'pc' in ins[0].op_str:
        lit = ((ins[0].address + 4) & ~3) + int(ins[0].op_str.split('#')[1].rstrip(']'), 16)
        x = u32(lit)
        k = 0
        for i in ins[1:3]:
            if i.mnemonic == 'bx':
                return emu_ram().r32(x + k), x + k
            if i.mnemonic in ('adds', 'subs') and i.op_str.startswith('r0, #'):
                v = int(i.op_str.split('#')[1], 16)
                k += v if i.mnemonic == 'adds' else -v
    vt, how, single = runtime_vtable(fn)
    return vt, single


def dump_ctrl(c, seen):
    if c in seen:
        return
    seen.add(c)
    cvt = emu_ram().r32(c)
    print(f'   controller {c:#x} vt {cvt:#x}')
    if 0x08000000 <= cvt < 0x08800000:
        for k, cf in enumerate(entries(cvt, 3)):
            print(f'   -- ctrl[{k}] {cf:08x}')
            print(full(cf, 160))


def main(args):
    for s in args:
        f = int(s, 16)
        print(f'################ node {f:08x}')
        seen = set()
        for st, a in defs_of(f):
            a = int(a, 16)
            if a in seen:
                continue
            seen.add(a)
            if len(seen) > 4:
                break
            print('  def', hex(a), 'state', st, [hex(u32(a + 4 * k)) for k in range(7)])
        print(full(f, 40))
        vt, single = node_vt(f)
        print('vtable', vt and hex(vt), 'singleton', single and hex(single))
        cs = set()
        if vt and 0x08000000 <= vt < 0x08800000:
            for slot, fn in enumerate(entries(vt, 6)):
                if fn in COMMON:
                    print(f'-- vt[{slot}] {fn:08x} = {COMMON[fn]}')
                    continue
                if slot in (0, 1, 2, 5):
                    print(f'-- vt[{slot}] {fn:08x}')
                    print(full(fn, 160))
                    for c in sorted(set(fn_features(fn)['ctrls'])):
                        dump_ctrl(c, cs)


if __name__ == '__main__':
    main(sys.argv[1:])
