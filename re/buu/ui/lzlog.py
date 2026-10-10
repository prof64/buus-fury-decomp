"""Log webfoot-LZ decompressions (IWRAM 0x03000040) while driving a state.

lzlog.py STATE [keys...]  (keys as in cap.py)
"""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/ui')
from common import *
from cap import drive, dump


def log_run(e, keys, frames_after=0):
    bp = e.breakpoint(0x03000040)
    out = []
    for k in keys + ([f'wait:{frames_after}'] if frames_after else []):
        drive(e, [k])
        for h in e.hits():
            r = h['regs']
            out.append((h['frame'], r[0], r[1], r[2], h['lr'], r[3]))
    e.clear(bp)
    return out


if __name__ == '__main__':
    e = emu(sys.argv[1])
    for f, r0, r1, r2, lr, r3 in log_run(e, sys.argv[2:]):
        t = e.r32(r0) if 0x08000000 <= r0 < 0x0a000000 else None
        sz = e.r32(r0 + 4) if t is not None else None
        print(f'frame {f} r0={r0:08x} r1={r1:08x} r2={r2:08x} r3={r3:08x} lr={lr:08x} type={t} size={sz}')
