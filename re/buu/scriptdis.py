"""Bytecode (Webfoot ByteCodeInterpreter) disassembler.

Interpreter: 0x08004038 runs ops from table 0x080554C0 (one handler per opcode byte):
  00 n8        push n8                     01 vint      push signed varint
  02 n8        call native[n8] (table 0x08055098; natives pop/push the VM stack)
  03 n8        pop -> setter[n8] (table 0x08055478)
  04 and 05 or 06 neg 07 add 08 sub 09 mul 0a div 1e mod
  0b eq 0c ne 0d gt 0e ge 0f lt 10 le       11 end (return)
  12 s16       jump (big-endian, relative to the operand address)
  13 s16       pop; jump if zero
  14           pop/discard
  15..19 n8    getter/setter combos on var n8 (inc/dec/...)
  1a           ?  1b push to loop stack   1c s16 loop (dec counter, jump back while nonzero)
  1d n8 vint*n push n varints
varint: 7-bit groups, MSB first, continuation bit 0x80; zigzag (odd => -(v>>1), even => v>>1).
Script table (subroutines run by native 0xD8 'call script(idx, p0, p1)'): 0x0879B704.
"""
import sys
from common import *

SCRIPTS = 0x0879B704
NAT_NAMES = {
    0xd5: 'is_equipped(item)', 0xd8: 'run_script(idx,p0,p1)',
    0xdf: 'eq_hp_regen+=n*0x445>>8', 0xe0: 'eq_thorns+=n*0xa4>>6', 0xe1: 'eq_END+END2+=n', 0xe2: 'eq_POW+=n',
    0xe3: 'eq_SPD+=n', 0xe4: 'eq_STR+=n', 0xe5: 'eq_block_dmg/=2', 0xe6: 'eq_energy_cost/=2',
    0xe7: 'eq_eblock_cost/=2', 0xe8: 'eq_ss_drain*=7/8', 0xf5: 'eq_byte0+=n',
}


def varint(b, i):
    v = 0
    while True:
        c = b[i]; i += 1
        v = (v << 7) | (c & 0x7f)
        if not c & 0x80:
            break
    return (-(v >> 1) if v & 1 else v >> 1), i


def script_bytes(idx_or_hdr, is_idx=True):
    """Return (code_bytes, base_address) for a script (decompressing if needed)."""
    hdr = u32(SCRIPTS + 4 * idx_or_hdr) if is_idx else idx_or_hdr
    blob = u32(hdr)
    comp = u8(blob + 2)
    if comp == 4:
        from decomp import decompress
        return decompress(blob + 4), 0
    return rom()[ra(blob + 3):ra(blob + 3) + 20000], blob + 3


def dis_script(addr, maxlen=4000, stop_at_end=True, data=None):
    b = data if data is not None else rom()[ra(addr):ra(addr) + maxlen]
    i = 0
    out = []
    names = {4: 'and', 5: 'or', 6: 'neg', 7: 'add', 8: 'sub', 9: 'mul', 0xa: 'div', 0x1e: 'mod', 0xb: 'eq', 0xc: 'ne',
             0xd: 'gt', 0xe: 'ge', 0xf: 'lt', 0x10: 'le', 0x14: 'pop', 0x1b: 'loop_push'}
    while i < len(b):
        a = addr + i
        op = b[i]; i += 1
        if op == 0:
            out.append((a, f'push {b[i]}')); i += 1
        elif op == 1:
            v, i = varint(b, i); out.append((a, f'push {v}'))
        elif op == 2:
            n = b[i]; i += 1; out.append((a, f'native 0x{n:02x} {NAT_NAMES.get(n, "")}'))
        elif op == 3:
            n = b[i]; i += 1; out.append((a, f'set 0x{n:02x}'))
        elif op == 0x11:
            out.append((a, 'end'))
            if stop_at_end:
                break
        elif op in (0x12, 0x13, 0x1c):
            off = (b[i] << 8) | b[i + 1]
            if op == 0x1c:
                tgt = a + 1 - off
            else:
                off = off - 0x10000 if off & 0x8000 else off
                tgt = a + 1 + off
            i += 2
            out.append((a, {0x12: 'jmp', 0x13: 'jz', 0x1c: 'loop'}[op] + f' {tgt:08x}'))
        elif op in (0x15, 0x16, 0x17, 0x18, 0x19, 0x1a):
            n = b[i]; i += 1; out.append((a, f'varop{op:02x} 0x{n:02x}'))
        elif op == 0x1d:
            n = b[i]; i += 1
            vals = []
            for _ in range(n):
                v, i = varint(b, i); vals.append(v)
            out.append((a, f'pushn {vals}'))
        elif op in names:
            out.append((a, names[op]))
        else:
            out.append((a, f'?? 0x{op:02x}'))
            break
    return out


def script_addr(idx):
    """Script table entry -> header struct whose first word points at the code blob.
    Blob: byte0,byte1 = ?, byte2 = 0 (raw code at +3) or 4 (compressed at +4)."""
    hdr = u32(SCRIPTS + 4 * idx)
    blob = u32(hdr)
    return blob, u8(blob + 2)


if __name__ == '__main__':
    arg = sys.argv[1]
    data = None
    if arg.startswith('#') or arg.startswith('h'):
        if arg.startswith('#'):
            blob, comp = script_addr(int(arg[1:]))
            data, a = script_bytes(int(arg[1:]))
        else:
            hdr = int(arg[1:], 16)
            blob, comp = u32(hdr), u8(u32(hdr) + 2)
            data, a = script_bytes(hdr, False)
        print(f'script {arg} blob {blob:08x} comp={comp}')
    else:
        a = int(arg, 16)
    for ad, s in dis_script(a, stop_at_end=len(sys.argv) < 3, data=data):
        print(f'  {ad:08x}: {s}')
