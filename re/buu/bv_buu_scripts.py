"""Disassemble the bytecode scripts queued by Super Buu 0x77's RunScript nodes (param = script header ptr)."""
import sys
from common import *
from scriptdis import dis_script, script_bytes, SCRIPTS

hdrs = [int(a, 16) for a in sys.argv[1:]] or [0x0879A690, 0x0879A7C4, 0x0879AA14]
idx = {u32(SCRIPTS + 4 * i): i for i in range(522)}
for h in hdrs:
    code, base = script_bytes(h, is_idx=False)
    print(f'#### script header {h:#x} = #{idx.get(h)} blob {u32(h):#x}')
    for off, t in dis_script(base, data=code[:600]):
        print('   ', hex(off), t)
