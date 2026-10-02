"""Shared helpers for Buu's Fury reverse engineering."""
import sys
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
from emu import Emu, KEY  # noqa: E402,F401

ROM_PATH = ROOT / 'roms' / 'buu.gba'
STATES = ROOT / 'work' / 'states'
WORK = ROOT / 'work' / 'buu'
WORK.mkdir(parents=True, exist_ok=True)
DATA = ROOT / 'data' / 'buu'
DATA.mkdir(parents=True, exist_ok=True)

_rom = None


def rom():
    global _rom
    if _rom is None:
        _rom = ROM_PATH.read_bytes()
    return _rom


def ra(addr):
    return addr - 0x08000000 if addr >= 0x08000000 else addr


def u8(a): return rom()[ra(a)]
def s8(a): return struct.unpack_from('<b', rom(), ra(a))[0]
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
        p = Path(state)
        if not p.is_absolute():
            p = STATES / state
        e.load_file(p)
    return e


_cs = None


def dis(addr, n=40, thumb=True):
    """Disassemble n instructions at addr (ROM)."""
    global _cs
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB if thumb else capstone.CS_MODE_ARM)
    a = addr & ~1
    code = rom()[ra(a):ra(a) + n * 4]
    out = []
    for i, ins in enumerate(md.disasm(code, a)):
        if i >= n:
            break
        extra = ''
        if ins.mnemonic == 'ldr' and 'pc' in ins.op_str:
            try:
                off = int(ins.op_str.split('#')[1].rstrip(']'), 16)
                lit = ((ins.address + 4) & ~3) + off
                extra = f'   ; =0x{u32(lit):08x}'
            except Exception:
                pass
        out.append(f'{ins.address:08x}: {ins.bytes.hex():10s} {ins.mnemonic} {ins.op_str}{extra}')
    return '\n'.join(out)


def disram(e, addr, n=40, thumb=True):
    import capstone
    md = capstone.Cs(capstone.CS_ARCH_ARM, capstone.CS_MODE_THUMB if thumb else capstone.CS_MODE_ARM)
    a = addr & ~1
    code = e.read(a, n * 4)
    return '\n'.join(f'{i.address:08x}: {i.mnemonic} {i.op_str}' for i in md.disasm(code, a))


class Shots:
    """Collect screenshots and write a grid image."""
    def __init__(self, e, name, cols=4):
        self.e, self.name, self.cols, self.ims = e, name, cols, []

    def snap(self):
        from PIL import Image
        self.ims.append(Image.frombytes('RGB', (240, 160), self.e.pixels()))

    def write(self):
        from PIL import Image
        n = len(self.ims)
        rows = (n + self.cols - 1) // self.cols
        W = Image.new('RGB', (240 * min(n, self.cols), 160 * rows))
        for i, im in enumerate(self.ims):
            W.paste(im, ((i % self.cols) * 240, (i // self.cols) * 160))
        p = WORK / f'{self.name}.png'
        W.save(p)
        return p
