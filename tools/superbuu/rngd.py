"""rngd.py s0 s1 t0 t1: number of RNG calls from state (s0, s1) to (t0, t1)"""
import sys
M = 0xFFFFFFFF


def step(s0, s1):
    a = ((s0 + 0x02CBBC71) & M) ^ s1
    b = ((((s1 << 3) | (s1 >> 29)) & M) + 0x0632D80F & M) ^ a
    return a, b


def dist(s, t, cap=100000):
    for n in range(cap):
        if s == t:
            return n
        s = step(*s)
    return None


if __name__ == '__main__':
    v = [int(x, 0) for x in sys.argv[1:5]]
    print(dist((v[0], v[1]), (v[2], v[3])))
