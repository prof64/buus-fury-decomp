"""linear disassembly of a range: go2.py START COUNT"""
import sys
sys.path.insert(0, '/Users/prof/source/buus-fury-decomp/re/buu/menu')
import d
a, n = int(sys.argv[1], 16), int(sys.argv[2])
for i in d.insns(a, n):
    print(f'{i.address:08x}: {i.mnemonic} {i.op_str}')
