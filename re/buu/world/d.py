"""Disassemble: d.py ADDR [N] [ADDR N ...]"""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu')
from common import *
args = sys.argv[1:]
i = 0
while i < len(args):
    a = int(args[i], 16)
    n = int(args[i + 1]) if i + 1 < len(args) and not args[i + 1].startswith('0') else 60
    i += 2 if i + 1 < len(args) and not args[i + 1].startswith('0') else 1
    print(f'--- {a:08x}')
    print(dis(a, n))
