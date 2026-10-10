"""Trace who runs the bytecode while native 0xB7 (modal script runner 0x08012588) blocks: log every entry of
the interpreter 0x08004038 (r1 = code pointer) with lr, and every native dispatch during the first frames."""
from common import *
from emutools import catch, cur_pc
from script_nat_c_lib import asm, load_code, call_keep, HDR_AT
import sys

items = [516, 1462, 1463, ('n', 0xb7)] if len(sys.argv) < 2 else [37, ('n', 0xb5)]
e = emu('buu_snakeway_start.ss')
e.run(2)
load_code(e, asm(items))
# install the call; it runs during the next frame and blocks
st = e.save()
log = []


def cb(e, pc):
    if pc == 0x08004038:
        log.append(f'f{e.frame} interp code={e.reg(1):08x} params={e.reg(2):08x} lr={e.reg(14):08x}')
    elif pc == 0x08003cbe:
        code = e.r32(e.reg(1))
        log.append(f'f{e.frame}   native {e.r8(code):02x} lr={e.reg(14):08x}')
    elif pc == 0x08012588:
        log.append(f'f{e.frame} modal_runner hdr={e.reg(0):08x} params={e.reg(1):08x} lr={e.reg(14):08x}')


r = call_keep(e, 0x08004098, (0, HDR_AT, 0, 0))
print('first call returned', hex(r))
n = catch(e, [0x08004038, 0x08003cbe, 0x08012588], 40, cb=cb)
print('\n'.join(log[:200]))
