"""Helpers for map RE: ROM access + capstone disassembly."""
import struct, sys
ROOT = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard'
sys.path.insert(0, ROOT + '/tools/extract')
from gba import Rom, wf_decompress
R = Rom('buu')
u8, u16, u32, s32 = R.u8, R.u16, R.u32, R.s32


def words(a, n):
    return [u32(a + 4 * i) for i in range(n)]


def dis(a, n=60):
    print(R.dis(a & ~1, n))


def hexw(a, n):
    print(' '.join(f'{w:08x}' for w in words(a, n)))


if __name__ == '__main__':
    cmd = sys.argv[1]
    a = int(sys.argv[2], 16)
    n = int(sys.argv[3]) if len(sys.argv) > 3 else 40
    if cmd == 'd':
        dis(a, n)
    elif cmd == 'w':
        hexw(a, n)
    elif cmd == 'lz':
        d, e = wf_decompress(R.d, a & 0x1FFFFFF)
        print(len(d), hex(e + 0x8000000), d[:n].hex())
