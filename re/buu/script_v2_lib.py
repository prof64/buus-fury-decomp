"""Round-2 native verification helpers (headless; no audio output).

* run(state, items, frames)        run bytecode through the game's runner (script_nat_b_lib.run_code), keep
                                   running until `frames` total frames; returns (emu, (frames, sp, stack)).
* diff(state, items, base, frames) differential RAM effect: run `items` and `base` (same pushes, natives
                                   replaced by pops) from the same state for the same number of frames; the
                                   emulator is deterministic, so the changed words are the native's effect.
* shot(e, name)                    screenshot -> work/buu/script/v2_<name>.png
"""
from common import *
from script_nat_b_lib import run_code, asm

OUT = WORK / 'script'
PLAYER_PTR = 0x03001C34
GD = 0x03001068


def run(state, items, frames=120, settle=3, keys=None, pre=None):
    e = emu(state)
    e.run(settle)
    if pre:
        pre(e)
    r = run_code(e, asm(items), max_frames=frames, keys=keys)
    done = r[0] or frames
    if done < frames:
        e.run(frames - done)
    return e, r


def pops(n):
    return [bytes([0x14])] * n


def ram(e):
    return e.ewram() + e.iwram()


def changed(a, b, skip=()):
    out = []
    for i in range(0, len(a), 4):
        if a[i:i + 4] != b[i:i + 4]:
            addr = (0x02000000 + i) if i < 0x40000 else (0x03000000 + i - 0x40000)
            if any(lo <= addr < hi for lo, hi in skip):
                continue
            out.append((addr, int.from_bytes(a[i:i + 4], 'little'), int.from_bytes(b[i:i + 4], 'little')))
    return out


SKIP = [(0x0203E000, 0x02040000), (0x03007000, 0x03008000)]


def diff(state, items, base, frames=60, pre=None, skip=SKIP):
    e1, r1 = run(state, items, frames, pre=pre)
    e0, r0 = run(state, base, frames, pre=pre)
    return changed(ram(e0), ram(e1), skip), r1, r0, e1, e0


def shot(e, name):
    p = OUT / f'v2_{name}.png'
    e.screenshot(str(p), scale=1)
    return p


def player(e):
    return e.r32(PLAYER_PTR)


def actor(e, i):
    from emutools import call
    return call(e, 0x080001C6, (i & 0xffffffff, 0, 0, 0))


def fmt(ch, n=12):
    return '; '.join(f'{a:08x}:{x:08x}->{y:08x}' for a, x, y in ch[:n]) + (f' (+{len(ch) - n})' if len(ch) > n else '')
