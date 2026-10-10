"""Emulator with savedata access (work/superbuu/libemu_sb.dylib, built by tools/superbuu/build_sb.sh from shim_sb.c = tools/emu/shim.c + savedata accessors)."""
import ctypes, sys
from pathlib import Path
HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = ROOT / 'work' / 'superbuu'   # libemu_sb.dylib (build_sb.sh)
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
sys.path.insert(0, str(ROOT / 're' / 'buu'))
import emu as _emu
_orig = _emu.HERE
def _load_sb():
    if _emu._lib is None:
        p = OUT / 'libemu_sb.dylib'
        # reuse emu._load's signature setup with our library
        save = _emu.HERE
        _emu.HERE = OUT
        try:
            import os
            L = None
            _emu._lib = None
            # emu._load looks for HERE/'libemu.dylib'
            tmp = OUT / 'libemu.dylib'
            if not tmp.exists():
                tmp.symlink_to(p.name)
            L = _emu._load()
        finally:
            _emu.HERE = save
        L.emu_savedata_type.argtypes = [ctypes.c_void_p]; L.emu_savedata_type.restype = ctypes.c_int
        L.emu_savedata_size.argtypes = [ctypes.c_void_p]; L.emu_savedata_size.restype = ctypes.c_size_t
        L.emu_savedata_read.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]; L.emu_savedata_read.restype = ctypes.c_size_t
        L.emu_savedata_write.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]; L.emu_savedata_write.restype = ctypes.c_size_t
    return _emu._lib
_load_sb()
import common  # noqa: E402  (uses emu.Emu -> our lib)

def sd_type(e): return e.L.emu_savedata_type(e.h)
def sd_read(e):
    n = e.L.emu_savedata_size(e.h)
    b = ctypes.create_string_buffer(max(n, 1))
    k = e.L.emu_savedata_read(e.h, b, n)
    return b.raw[:k]
def sd_write(e, data):
    return e.L.emu_savedata_write(e.h, data, len(data))
