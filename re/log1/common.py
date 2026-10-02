"""Shared helpers for LoG1 reverse engineering."""
import sys
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
from emu import Emu, KEY  # noqa: E402

ROM_PATH = ROOT / 'roms' / 'log1.gba'
STATES = ROOT / 'work' / 'states'
SHOTS = ROOT / 'work' / 'shots' / 'log1'
SHOTS.mkdir(parents=True, exist_ok=True)
DATA = ROOT / 'data' / 'log1'
DATA.mkdir(parents=True, exist_ok=True)

_rom = None


def rom():
    global _rom
    if _rom is None:
        _rom = ROM_PATH.read_bytes()
    return _rom


def ra(addr):
    """ROM offset from GBA address."""
    return addr - 0x08000000 if addr >= 0x08000000 else addr


def u8(a): return rom()[ra(a)]
def u16(a): return struct.unpack_from('<H', rom(), ra(a))[0]
def s16(a): return struct.unpack_from('<h', rom(), ra(a))[0]
def u32(a): return struct.unpack_from('<I', rom(), ra(a))[0]
def s32(a): return struct.unpack_from('<i', rom(), ra(a))[0]


def cstr(a, maxlen=256):
    b = rom()[ra(a):ra(a) + maxlen]
    return b.split(b'\0')[0].decode('latin-1')


def emu(state=None):
    e = Emu(str(ROM_PATH))
    if state:
        e.load_file(STATES / state if not str(state).startswith('/') else state)
    return e


_cs = None


def dis(addr, n=40, thumb=True):
    """Disassemble from ROM, returns text."""
    global _cs
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB if thumb else capstone.CS_MODE_ARM)
    code = rom()[ra(addr):ra(addr) + n * 4]
    out = []
    for i, ins in enumerate(md.disasm(code, addr)):
        if i >= n:
            break
        extra = ''
        if ins.mnemonic == 'ldr' and 'pc' in ins.op_str and '[' in ins.op_str:
            try:
                imm = int(ins.op_str.split('#')[-1].rstrip(']'), 0)
                lit = ((ins.address + 4) & ~3) + imm
                extra = f'   ; ={u32(lit):#010x}'
            except Exception:
                pass
        out.append(f'{ins.address:08x}: {ins.bytes.hex():<8} {ins.mnemonic:<6} {ins.op_str}{extra}')
    return '\n'.join(out)
