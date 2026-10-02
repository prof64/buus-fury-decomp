"""Emulator helpers: run-until-pc, call injection."""
from common import *


def run_to_pc(e, target, max_frames=6000, mash=0):
    """Advance until the CPU is about to execute `target` (thumb or arm). Returns True on success.
    Uses a breakpoint to find the frame, then reloads and single-steps.
    mash: key mask pressed for 4 of every 30 frames while waiting."""
    for f in range(max_frames):
        if mash:
            e.set_keys(mash if f % 30 < 4 else 0)
        st = e.save()
        bid = e.breakpoint(target)
        e.run(1)
        hs = e.hits()
        e.clear(bid)
        if hs:
            e.load(st)
            for _ in range(5_000_000):
                if cur_pc(e) == target:
                    return True
                e.step()
            return False
    return False


def catch(e, addrs, frames, keyfn=lambda f: 0, cb=None):
    """Run `frames` frames with keys keyfn(frame_index). Whenever a frame executes any of `addrs`,
    replay that frame instruction-by-instruction and call cb(e, pc) when about to execute one of
    them (memory is live, so the callback can read structs). Returns number of callbacks."""
    addrs = set(a & ~1 for a in addrs)
    n = 0
    for f in range(frames):
        e.set_keys(keyfn(f))
        st = e.save()
        bids = [e.breakpoint(a) for a in addrs]
        e.run(1)
        hs = e.hits()
        for b in bids:
            e.clear(b)
        if hs:
            e.load(st)
            e.set_keys(keyfn(f))
            start = e.frame
            while e.frame == start:
                p = cur_pc(e)
                if p in addrs:
                    cb(e, p)
                    n += 1
                e.step()
            # finish: we are at the start of next frame now
    return n


STUB = 0x082ED300
ARGS = 0x082ED400
HOOK_SITE = 0x08007052      # BL 0x0800965c in the main loop (game-state update, once per frame)
HOOK_TARGET = 0x0800965c
MAILBOX = 0x0203FFF0


def _bl(src, tgt):
    off = tgt - (src + 4)
    return (0xF000 | ((off >> 12) & 0x7FF)), (0xF800 | ((off >> 1) & 0x7FF))


def call(e, func, r=(0, 0, 0, 0), stack=(0, 0, 0, 0)):
    """Inject a call func(r0..r3, stack args) at the start of the next game-state update.
    Runs exactly one frame. Returns r0 of the call."""
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
    e.run(1)
    e.w16(HOOK_SITE, o1); e.w16(HOOK_SITE + 2, o2)
    for i in range(0, len(blob), 2):
        e.w16(STUB + i, u16(STUB + i))
    for i in range(len(vals)):
        e.w32(ARGS + 4 * i, u32(ARGS + 4 * i))
    return e.r32(MAILBOX)


def cur_pc(e):
    """Address of the next instruction to execute (mGBA keeps gprs[15] = next + 2 (thumb) / + 4 (arm))."""
    pc = e.reg(15)
    return pc - (2 if e.reg(16) & 0x20 else 4)
