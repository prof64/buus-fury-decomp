"""Helpers: run until execution reaches a ROM address, with state *paused there* (via single stepping)."""
from common import *

def pc_of(e):
    return e.reg(15)

def run_to(e, addr, max_frames=600, keys=None, off=None):
    """Advance until instruction at addr is about to execute. Returns True if reached.
    keys: optional callable(frame_index)->keymask applied each frame."""
    for f in range(max_frames):
        s=e.save()
        if keys: e.set_keys(keys(f))
        bp=e.breakpoint(addr)
        e.run(1)
        hs=e.hits()
        e.clear(bp)
        if hs:
            target=hs[0]['regs'][15]
            e.load(s)
            if keys: e.set_keys(keys(f))
            n=0
            while e.reg(15)!=target:
                e.step(); n+=1
                if n>2_000_000: raise RuntimeError('step overflow')
            return True
    return False

def step_until(e, addr_set, limit=2_000_000):
    n=0
    while e.reg(15) not in addr_set:
        e.step(); n+=1
        if n>limit: return False
    return True
