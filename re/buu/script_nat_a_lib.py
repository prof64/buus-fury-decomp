"""Helpers for decoding script natives 0x00..0x57: script enumeration (only blob kind 0/4 are scripts),
reachable-code disassembly (self-delimiting opcodes; follows jz/jmp/loop), native use counts and argument
contexts, and an emulator harness that runs arbitrary bytecode through the game's own runner 0x08004098."""
from collections import defaultdict
from common import *
from scriptdis import varint

SCRIPTS = 0x0879B704
NSCRIPTS = 522


def script_code(i):
    h = u32(SCRIPTS + 4 * i)
    b = u32(h)
    kind = u8(b + 2)
    if kind == 0:
        return rom()[ra(b + 3):ra(b + 3) + 0x4000], b + 3
    if kind == 4:
        from decomp import decompress
        return decompress(b + 4), 0
    return None, None


def walk(code):
    """Reachable instructions: list of (off, op, operand)."""
    seen = {}
    work = [0]
    while work:
        i = work.pop()
        while i < len(code) and i not in seen:
            a = i
            op = code[i]; i += 1
            arg = None
            if op in (0, 2, 3) or 0x15 <= op <= 0x1a:
                arg = code[i]; i += 1
            elif op == 1:
                arg, i = varint(code, i)
            elif op == 0x1d:
                n = code[i]; i += 1
                arg = []
                for _ in range(n):
                    v, i = varint(code, i); arg.append(v)
            elif op in (0x12, 0x13, 0x1c):
                off = (code[i] << 8) | code[i + 1]
                arg = (a + 1 - off) if op == 0x1c else (a + 1 + off)
                i += 2
            elif op > 0x1e:
                seen[a] = ('bad', op, None)
                break
            seen[a] = (op, arg, i)
            if op == 0x11:
                break
            if op == 0x12:
                i = arg
                continue
            if op in (0x13, 0x1c):
                work.append(arg)
    return [(a,) + seen[a] for a in sorted(seen)]


def all_scripts():
    out = {}
    for i in range(NSCRIPTS):
        c, base = script_code(i)
        if c is None:
            continue
        out[i] = walk(c)
    return out


def native_contexts(scripts=None):
    """{native: [(script, off, preceding pushes list)]}"""
    scripts = scripts or all_scripts()
    ctx = defaultdict(list)
    for s, ins in scripts.items():
        for k, (a, op, arg, nxt) in enumerate(ins):
            if op == 2:
                pre = []
                j = k - 1
                while j >= 0 and len(pre) < 6:
                    o, ar = ins[j][1], ins[j][2]
                    if o in (0, 1):
                        pre.insert(0, ar)
                    elif o == 0x1d:
                        pre = list(ar) + pre
                    elif o == 2:
                        pre.insert(0, f'n{ar:02x}()')
                    else:
                        break
                    j -= 1
                ctx[arg].append((s, a, pre))
    return ctx


# ---- emulator harness ---------------------------------------------------------------------------
CODE_AT = 0x0203F000
HDR_AT = 0x0203EFF0


def vint(v):
    z = (-v << 1) | 1 if v < 0 else v << 1
    groups = []
    while True:
        groups.insert(0, z & 0x7F)
        z >>= 7
        if not z:
            break
    return bytes([g | 0x80 for g in groups[:-1]] + [groups[-1]])


def asm(items):
    """items: ints (push), ('n', id) native, ('s', id) setter, raw bytes."""
    b = bytearray()
    for it in items:
        if isinstance(it, int):
            b += b'\x01' + vint(it)
        elif isinstance(it, (bytes, bytearray)):
            b += it
        elif it[0] == 'n':
            b += bytes([2, it[1]])
        elif it[0] == 's':
            b += bytes([3, it[1]])
    b += b'\x11'
    return bytes(b)


def run_code(e, code, params=0):
    """Run raw bytecode through the game's runner. Returns (interp_ptr, sp, stack values) or None if it
    did not return within the frame (blocked)."""
    from emutools import call
    blob = b'\x01\x00\x00' + code
    blob += b'\x00' * (-len(blob) % 4)
    for i in range(0, len(blob), 4):
        e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
    e.w32(HDR_AT, CODE_AT)
    r = call(e, 0x08004098, (0, HDR_AT, params, 0))
    if r == 0xDEADBEEF:
        return None
    sp = e.r32(r)
    st = [e.r32(r + 4 + 4 * k) for k in range(max(0, min(sp, 16)))]
    return r, sp, st
