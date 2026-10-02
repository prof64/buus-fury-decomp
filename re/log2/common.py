import sys, struct
ROOT='/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/'
sys.path.insert(0,ROOT+'tools/emu')
from emu import Emu, KEY
W=ROOT+'work/'
ST=W+'states/'
OUT=W+'log2/'
ROM=open(ROOT+'roms/log2.gba','rb').read()
def u8(a): return ROM[a&0x1ffffff]
def u16(a): return struct.unpack_from('<H',ROM,a&0x1ffffff)[0]
def u32(a): return struct.unpack_from('<I',ROM,a&0x1ffffff)[0]
def s16(a): return struct.unpack_from('<h',ROM,a&0x1ffffff)[0]
def s32(a): return struct.unpack_from('<i',ROM,a&0x1ffffff)[0]
def new(state=None):
    e=Emu('log2')
    if state: e.load_file(ST+state if '/' not in state else state)
    return e
from capstone import Cs, CS_ARCH_ARM, CS_MODE_THUMB, CS_MODE_ARM
_ct=Cs(CS_ARCH_ARM, CS_MODE_THUMB); _ca=Cs(CS_ARCH_ARM, CS_MODE_ARM)
def dis(addr, n=40, arm=False):
    off=addr&0x1ffffff
    md=_ca if arm else _ct
    out=[]
    for i in md.disasm(ROM[off:off+n*4], addr):
        s=f'{i.address:08x}: {i.mnemonic} {i.op_str}'
        # resolve ldr pc-relative
        if i.mnemonic=='ldr' and 'pc' in i.op_str and '#' in i.op_str:
            try:
                imm=int(i.op_str.split('#')[1].rstrip(']'),0)
                la=((i.address+4)&~3)+imm if not arm else i.address+8+imm
                s+=f'   ; ={u32(la):#x}'
            except Exception: pass
        out.append(s)
        if len(out)>=n: break
    return '\n'.join(out)
