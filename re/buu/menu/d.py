"""d.py -- function disassembler for the menu RE.

d.py ADDR [ADDR...]       disassemble whole functions (linear sweep until the last branch target is passed and a
                          return is reached), literal pools resolved, bl targets named (NAMES below)
d.py vt VTADDR [N]        list a relative vtable (fn = vt + entry)
"""
import sys, struct, re
sys.path.insert(0, __file__.rsplit('/', 2)[0])
from common import rom, u32, u16, ra
import capstone

NAMES = {
    0x0803D8A0: 'run_mode', 0x0803D5B8: 'alloc', 0x08046AA2: 'call_r1', 0x08046AA4: 'call_r2', 0x08046AA6: 'call_r3',
    0x0803E378: 'play_sfx', 0x08011DB4: 'world_save', 0x08011E0E: 'world_restore', 0x0803F444: 'font_glyph',
    0x0803DADA: 'draw_string', 0x0803FFE8: 'measure', 0x0804001A: 'measure2', 0x08004F1C: 'menu_bg',
    0x08004F50: 'panel', 0x0803DE6A: 'line_fit', 0x0803F922: 'blit', 0x03000040: 'lz_decomp',
    0x0803FAAC: 'rand', 0x08024BD0: 'load_ui_palette', 0x08006718: 'status_page',
}
_md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB)


def name(a):
    a &= ~1
    return NAMES.get(a, f'sub_{a:08x}')


def insns(a, n=4000):
    code = rom()[ra(a):ra(a) + n * 2]
    return _md.disasm(code, a)


def func(a, maxn=3000, quiet=False):
    a &= ~1
    out = []
    far = a
    end = None
    lits = set()
    pos = a
    count = 0
    while count < maxn:
        if pos in lits:
            # skip literal word
            out.append(f'{pos:08x}: .word 0x{u32(pos):08x}')
            pos += 4
            continue
        b = rom()[ra(pos):ra(pos) + 4]
        got = list(_md.disasm(b, pos, 1))
        if not got:
            out.append(f'{pos:08x}: .hword 0x{u16(pos):04x}')
            pos += 2
            continue
        ins = got[0]
        count += 1
        extra = ''
        m, op = ins.mnemonic, ins.op_str
        if m == 'ldr' and 'pc' in op:
            try:
                off = int(op.split('#')[1].rstrip(']'), 16)
            except Exception:
                off = 0
            lit = ((ins.address + 4) & ~3) + off
            lits.add(lit)
            v = u32(lit)
            extra = f'   ; =0x{v:08x}'
            if 0x08000000 <= v < 0x08800000 and (v & 1):
                extra += f' ({name(v)})'
        elif m == 'bl':
            t = int(op.lstrip('#'), 16)
            extra = f'   ; {name(t)}'
        elif m.startswith('b') and m not in ('bl', 'bx', 'blx', 'bic', 'bics'):
            try:
                t = int(op.lstrip('#'), 16)
                if t > far:
                    far = t
            except Exception:
                pass
        elif m == 'add' and 'pc' in op:
            pass
        out.append(f'{ins.address:08x}: {m} {op}{extra}')
        pos = ins.address + ins.size
        ret = (m == 'pop' and 'pc' in op) or (m == 'bx' and 'lr' in op) or (m == 'b' and int(op.lstrip('#'), 16) < pos and pos > far)
        if m == 'bx' and op != 'lr' and pos > far:
            ret = True
        if ret and pos > far:
            # dump trailing literal pool
            while pos in lits or (pos % 4 and pos + 2 in lits):
                if pos in lits:
                    out.append(f'{pos:08x}: .word 0x{u32(pos):08x}')
                    pos += 4
                else:
                    pos += 2
            break
    if not quiet:
        print(f'=== {name(a)} @ {a:08x}')
        print('\n'.join(out))
    return out


def vt(v, n=16):
    for i in range(n):
        e = u32(v + 4 * i)
        f = (v + e) & 0xFFFFFFFF
        print(f'  [{i:2d}] +0x{4*i:02x} -> {f:08x}')


if __name__ == '__main__':
    args = sys.argv[1:]
    if args and args[0] == 'vt':
        vt(int(args[1], 16), int(args[2]) if len(args) > 2 else 16)
    else:
        for s in args:
            func(int(s, 16))
