"""cmpstate.py a.ss b.ss start len -- word diff two states in a RAM range"""
import sys
from common import *


def load(n):
    return emu(n if (STATES / n).exists() else WORK / n)


a, b = load(sys.argv[1]), load(sys.argv[2])
s = int(sys.argv[3], 16); n = int(sys.argv[4], 16)
A, B = a.read(s, n), b.read(s, n)
for i in range(0, n, 4):
    x, y = int.from_bytes(A[i:i+4], 'little'), int.from_bytes(B[i:i+4], 'little')
    if x != y:
        print(f'{s+i:08x}: {x:08x} {y:08x}')
