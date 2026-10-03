"""emut.py -- tools/emu's Emu over libemut.dylib: breakpoint / watchpoint hits carry the global cycle counter
(`mTimingGlobalTime`) and VCOUNT, and `cycles()` reads the counter (for measuring the game's CPU time)."""
import ctypes
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
import emu as base  # noqa: E402


class HitT(ctypes.Structure):
    _fields_ = [
        ('kind', ctypes.c_uint32), ('address', ctypes.c_uint32),
        ('old', ctypes.c_uint32), ('new', ctypes.c_uint32),
        ('access', ctypes.c_uint32), ('width', ctypes.c_uint32),
        ('frame', ctypes.c_uint32), ('regs', ctypes.c_int32 * 16), ('cpsr', ctypes.c_uint32),
        ('vcount', ctypes.c_uint32), ('cycles', ctypes.c_uint64),
    ]

    def as_dict(self):
        regs = [r & 0xFFFFFFFF for r in self.regs]
        return {'kind': 'watch' if self.kind == 1 else 'break', 'addr': self.address, 'old': self.old, 'new': self.new,
                'width': self.width, 'frame': self.frame, 'regs': regs, 'pc': regs[15], 'lr': regs[14],
                'vcount': self.vcount, 'cycles': self.cycles}


_lib = None


def _load():
    global _lib
    if _lib is None:
        path = HERE / 'libemut.dylib'
        if not path.exists():
            import os
            os.system(f'sh {HERE / "build.sh"}')
        # same symbols as libemu; reuse the base loader's signatures on our library
        saved = base._lib
        base._lib = None
        orig = base.HERE
        try:
            base.HERE = HERE
            lib_path = HERE / 'libemu.dylib'
            # base._load looks for HERE/libemu.dylib: point it at ours through a symlink-free copy of the logic
            L = ctypes.CDLL(str(path))
            base._lib = L
            # re-run the signature setup by calling base._load with _lib preset is not possible; set them here
            P = ctypes.c_void_p
            u32 = ctypes.c_uint32
            L.emu_open.restype = P
            L.emu_open.argtypes = [ctypes.c_char_p]
            for name in ('emu_read8', 'emu_read16', 'emu_read32'):
                getattr(L, name).restype = u32
                getattr(L, name).argtypes = [P, u32]
            L.emu_write8.argtypes = [P, u32, ctypes.c_uint8]
            L.emu_write16.argtypes = [P, u32, ctypes.c_uint16]
            L.emu_write32.argtypes = [P, u32, u32]
            L.emu_read_block.argtypes = [P, u32, ctypes.c_char_p, u32]
            L.emu_run_frames.argtypes = [P, ctypes.c_int]
            L.emu_step.argtypes = [P]
            L.emu_set_keys.argtypes = [P, u32]
            L.emu_frame.argtypes = [P]
            L.emu_frame.restype = u32
            L.emu_reg.argtypes = [P, ctypes.c_int]
            L.emu_reg.restype = ctypes.c_int32
            L.emu_set_reg.argtypes = [P, ctypes.c_int, ctypes.c_int32]
            L.emu_video.argtypes = [P]
            L.emu_video.restype = ctypes.POINTER(u32)
            L.emu_state_size.argtypes = [P]
            L.emu_state_size.restype = ctypes.c_size_t
            L.emu_save_state.argtypes = [P, ctypes.c_char_p]
            L.emu_load_state.argtypes = [P, ctypes.c_char_p]
            L.emu_watch.argtypes = [P, u32, u32, ctypes.c_int]
            L.emu_break.argtypes = [P, u32]
            L.emu_clear.argtypes = [P, ctypes.c_int]
            L.emu_hits.argtypes = [P, ctypes.POINTER(ctypes.POINTER(HitT)), ctypes.POINTER(u32)]
            L.emu_hits.restype = u32
            L.emu_clear_hits.argtypes = [P]
            L.emu_close.argtypes = [P]
            L.emu_set_volume.argtypes = [P, ctypes.c_int]
            L.emu_cycles.argtypes = [P]
            L.emu_cycles.restype = ctypes.c_uint64
            _lib = L
        finally:
            base.HERE = orig
            base._lib = saved
    return _lib


class EmuT(base.Emu):
    def __init__(self, rom='buu'):
        self.L = _load()
        path = str(base.ROMS.get(rom, rom))
        self.h = self.L.emu_open(path.encode())
        if not self.h:
            raise RuntimeError(f'cannot open {path}')
        self.keys = 0

    def hits(self, clear=True):
        ptr = ctypes.POINTER(HitT)()
        dropped = ctypes.c_uint32()
        n = self.L.emu_hits(self.h, ctypes.byref(ptr), ctypes.byref(dropped))
        out = [ptr[i].as_dict() for i in range(n)]
        if clear:
            self.L.emu_clear_hits(self.h)
        return out

    def cycles(self):
        return self.L.emu_cycles(self.h)


def emut(state=None):
    e = EmuT(str(ROOT / 'roms' / 'buu.gba'))
    if state:
        p = Path(state)
        if not p.is_absolute():
            p = ROOT / 'work' / 'states' / state
        e.load_file(p)
    return e
