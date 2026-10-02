"""Emulator harness for natives 0x58..0xA7: run arbitrary bytecode through the game's runner 0x08004098 and
let BLOCKING natives (nested main loops) run to completion.

The standard emutools.call restores the hooked main-loop BL after exactly one frame, which breaks when the
called code enters a nested main loop. Here the hook is removed as soon as the stub is entered (found by
single-stepping), so re-entry is impossible and the call may span any number of frames; completion is
detected by the stub's mailbox write."""
from common import *
from emutools import STUB, ARGS, HOOK_SITE, HOOK_TARGET, MAILBOX, _bl, cur_pc

CODE_AT = 0x0203F000
HDR_AT = 0x0203EFF0
INTERP_AT = 0x0203EF00   # 0x6c bytes interp context we hand to the runner


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
    """items: int -> push varint; ('n', id) native; ('s', id) setter; bytes raw."""
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


def start_call(e, func, r=(0, 0, 0, 0), stack=(0, 0, 0, 0)):
    blob = (ROOT / 'work' / 'buu' / 'stub.bin').read_bytes()
    for i in range(0, len(blob), 2):
        e.w16(STUB + i, blob[i] | (blob[i + 1] << 8))
    vals = list(r) + list(stack) + [func | 1, HOOK_TARGET | 1]
    for i, v in enumerate(vals):
        e.w32(ARGS + 4 * i, v)
    hi, lo = _bl(HOOK_SITE, STUB)
    o1, o2 = u16(HOOK_SITE), u16(HOOK_SITE + 2)
    e.w32(MAILBOX, 0xDEADBEEF)
    e.w16(HOOK_SITE, hi)
    e.w16(HOOK_SITE + 2, lo)
    # run until the stub is entered, then un-hook
    st = e.save()
    bid = e.breakpoint(STUB)
    for _ in range(10):
        e.run(1)
        if e.hits():
            break
        st = e.save()
    e.clear(bid)
    e.load(st)
    for _ in range(3_000_000):
        if cur_pc(e) == STUB:
            break
        e.step()
    e.w16(HOOK_SITE, o1)
    e.w16(HOOK_SITE + 2, o2)


def run_code(e, code, params=0, max_frames=600, per_frame=None, keys=None):
    """Run bytecode; returns (frames_taken or None if still blocked, sp, stack)."""
    blob = b'\x01\x00\x00' + code
    blob += b'\x00' * (-len(blob) % 4)
    for i in range(0, len(blob), 4):
        e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
    e.w32(HDR_AT, CODE_AT)
    for i in range(0, 0x6c, 4):
        e.w32(INTERP_AT + i, 0)
    start_call(e, 0x08004098, (INTERP_AT, HDR_AT, params, 0))
    for f in range(max_frames):
        if keys:
            e.set_keys(keys(f))
        e.run(1)
        if per_frame:
            per_frame(e, f)
        if e.r32(MAILBOX) != 0xDEADBEEF:
            e.set_keys(0)
            sp = e.r32(INTERP_AT)
            return f + 1, sp, [e.r32(INTERP_AT + 4 + 4 * k) for k in range(min(sp, 16))]
    e.set_keys(0)
    return None, None, None
