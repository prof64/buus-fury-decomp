"""Which resource-kind handler (table 0x080728AC, indexed by blob byte 2) runs for a modal resource, and from
where it is dispatched. usage: script_nat_c_t4.py idx [native]"""
import sys
from common import *
from emutools import catch
from script_nat_c_lib import asm, load_code, call_keep, HDR_AT

idx = int(sys.argv[1])
nat = int(sys.argv[2], 16) if len(sys.argv) > 2 else 0xb5
H = [u32(0x080728AC + 4 * k) & ~1 for k in range(8)]
e = emu('buu_snakeway_start.ss')
e.run(2)
load_code(e, asm([idx, ('n', nat)]))
log = []


def cb(e, pc):
    k = H.index(pc)
    log.append(f'f{e.frame} kind{k} handler {pc:08x} thread={e.reg(0):08x} blob={e.reg(1):08x} lr={e.reg(14):08x}')


call_keep(e, 0x08004098, (0, HDR_AT, 0, 0))
catch(e, H, 30, cb=cb)
print('\n'.join(log) or 'no handler hit')
