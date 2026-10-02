"""Emulator harness for natives 0xA8..0xF7: run raw bytecode through the game's runner 0x08004098 at the
per-frame hook (emutools.call) and keep running frames while it blocks."""
from common import *
from emutools import call, STUB, ARGS, HOOK_SITE, MAILBOX, _bl

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


def load_code(e, code):
    blob = b'\x01\x00\x00' + code
    blob += b'\x00' * (-len(blob) % 4)
    for i in range(0, len(blob), 4):
        e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
    e.w32(HDR_AT, CODE_AT)


def call_keep(e, func, r=(0, 0, 0, 0), stack=(0, 0, 0, 0)):
    """Like emutools.call but leaves the stub/args in place (the call may block across frames via a nested
    main loop); only the hook site is restored after one frame."""
    blob = (ROOT / 'work' / 'buu' / 'stub.bin').read_bytes()
    for i in range(0, len(blob), 2):
        e.w16(STUB + i, blob[i] | (blob[i + 1] << 8))
    vals = list(r) + list(stack) + [func | 1, 0x0800965c | 1]
    for i, v in enumerate(vals):
        e.w32(ARGS + 4 * i, v)
    hi, lo = _bl(HOOK_SITE, STUB)
    o1, o2 = e.r16(HOOK_SITE), e.r16(HOOK_SITE + 2)
    e.w32(MAILBOX, 0xDEADBEEF)
    e.w16(HOOK_SITE, hi); e.w16(HOOK_SITE + 2, lo)
    e.run(1)
    e.w16(HOOK_SITE, o1); e.w16(HOOK_SITE + 2, o2)
    return e.r32(MAILBOX)


def start(e, code, params=0):
    """Install the stub so the runner is entered at the next frame; returns after 1 frame. Returns
    (done, interp, stack). If not done, the script blocked; call finish(e, n) to keep running."""
    load_code(e, code)
    r = call_keep(e, 0x08004098, (0, HDR_AT, params, 0))
    if r == 0xDEADBEEF:
        return False, None, None
    sp = e.r32(r)
    return True, r, [e.r32(r + 4 + 4 * k) for k in range(max(0, min(sp, 16)))]


def run_until_done(e, frames, keys=0, every=None, shots=None):
    """Keep running while the injected call is blocked; mailbox != 0xDEADBEEF => returned."""
    for f in range(frames):
        e.set_keys(keys(f) if callable(keys) else keys)
        e.run(1)
        if shots is not None and every and f % every == 0:
            shots.snap()
        if e.r32(MAILBOX) != 0xDEADBEEF:
            return f + 1
    return None
