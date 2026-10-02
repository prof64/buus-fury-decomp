"""Buu's Fury PRNG (0x0803FAAC), state at IWRAM 0x03004A5C (s0) / 0x03004A60 (s1).

    a  = (s0 + 0x02CBBC71) ^ s1
    b  = (rol32(s1, 3) + 0x0632D80F) ^ a
    s0 = a; s1 = b; return (a - b) mod 2^32
rand(n)  (0x0803FACC) = ((next() & 0xFFFF) * n) >> 16
chance(p) (0x0803FADE) = (next() & 0xFFFF) < p
"""
M = 0xFFFFFFFF


class Rng:
    def __init__(self, s0, s1):
        self.s0, self.s1 = s0 & M, s1 & M

    def next(self):
        a = ((self.s0 + 0x02CBBC71) & M) ^ self.s1
        b = ((((self.s1 << 3) | (self.s1 >> 29)) & M) + 0x0632D80F) & M
        b ^= a
        self.s0, self.s1 = a, b
        return (a - b) & M

    def randn(self, n):
        return ((self.next() & 0xFFFF) * n) >> 16

    def chance(self, p):
        return (self.next() & 0xFFFF) < p
