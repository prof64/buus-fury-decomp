"""Emulator harness for natives that block (run a nested main loop): inject the runner call once (hook patched
only until the stub is entered), then keep running frames until the runner returns, calling `on_frame(e, i)`
each frame. Also helpers to list world objects."""
from common import *
from emutools import STUB, ARGS, HOOK_SITE, HOOK_TARGET, MAILBOX, _bl, run_to_pc
from script_nat_a_lib import CODE_AT, HDR_AT

WORLD = 0x03001C4C
PLAYER_PTR = 0x03001C34


def start_call(e, func, r=(0, 0, 0, 0), stack=(0, 0, 0, 0)):
    blob = (ROOT / 'work' / 'buu' / 'stub.bin').read_bytes()
    for i in range(0, len(blob), 2):
        e.w16(STUB + i, blob[i] | (blob[i + 1] << 8))
    vals = list(r) + list(stack) + [func | 1, HOOK_TARGET | 1]
    for i, v in enumerate(vals):
        e.w32(ARGS + 4 * i, v)
    hi, lo = _bl(HOOK_SITE, STUB)
    o1, o2 = e.r16(HOOK_SITE), e.r16(HOOK_SITE + 2)
    e.w32(MAILBOX, 0xDEADBEEF)
    e.w16(HOOK_SITE, hi); e.w16(HOOK_SITE + 2, lo)
    ok = run_to_pc(e, STUB, max_frames=5)
    e.w16(HOOK_SITE, o1); e.w16(HOOK_SITE + 2, o2)
    return ok


def run_blocking(e, code, max_frames=600, on_frame=None, params=0):
    """Run bytecode via the runner; returns (frames_taken, interp_ptr) or (None, None) on timeout."""
    blob = b'\x01\x00\x00' + code
    blob += b'\x00' * (-len(blob) % 4)
    for i in range(0, len(blob), 4):
        e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
    e.w32(HDR_AT, CODE_AT)
    start_call(e, 0x08004098, (0, HDR_AT, params, 0))
    for f in range(max_frames):
        if e.r32(MAILBOX) != 0xDEADBEEF:
            return f, e.r32(MAILBOX)
        e.run(1)
        if on_frame:
            on_frame(e, f)
    if e.r32(MAILBOX) != 0xDEADBEEF:
        return max_frames, e.r32(MAILBOX)
    return None, None


def world_objects(e):
    n = e.r32(WORLD + 4)
    return [e.r32(WORLD + 8 + 4 * i) for i in range(min(n, 256))]


def pos(e, o):
    def s(v):
        return v - (1 << 32) if v & 0x80000000 else v
    return s(e.r32(o + 0x1A0)) / 256, s(e.r32(o + 0x1A4)) / 256
