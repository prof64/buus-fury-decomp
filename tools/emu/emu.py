"""Headless GBA emulator for reverse engineering (ctypes over tools/emu/libemu.dylib).

    from emu import Emu, KEY
    e = Emu('log2')            # 'log1' | 'log2' | 'buu' or a path
    e.run(300)                 # run frames
    e.press(KEY.A, frames=4)   # hold a button then release
    e.r32(0x03000000)          # raw reads (no side effects)
    e.screenshot('out.png')
    s = e.save(); e.load(s)    # save states (bytes)
    e.watch(0x02001000, 0x02001004, 'w'); e.run(60); e.hits()  # writes + regs (pc = r15)
    e.breakpoint(0x08001234); e.run(1); e.hits()                 # slow while set
    e.set_volume(0x100); pcm = e.run_audio(60)   # int16 stereo @ e.audio_rate
"""
import ctypes
import os
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
ROMS = {
    'log1': ROOT / 'roms' / 'log1.gba',
    'log2': ROOT / 'roms' / 'log2.gba',
    'buu': ROOT / 'roms' / 'buu.gba',
}


class KEY:
    A, B, SELECT, START, RIGHT, LEFT, UP, DOWN, R, L = (1 << i for i in range(10))


class Hit(ctypes.Structure):
    _fields_ = [
        ('kind', ctypes.c_uint32), ('address', ctypes.c_uint32),
        ('old', ctypes.c_uint32), ('new', ctypes.c_uint32),
        ('access', ctypes.c_uint32), ('width', ctypes.c_uint32),
        ('frame', ctypes.c_uint32), ('regs', ctypes.c_int32 * 16), ('cpsr', ctypes.c_uint32),
    ]

    def as_dict(self):
        regs = [r & 0xFFFFFFFF for r in self.regs]
        return {
            'kind': 'watch' if self.kind == 1 else 'break', 'addr': self.address,
            'old': self.old, 'new': self.new, 'access': {1: 'w', 2: 'r'}.get(self.access, self.access),
            'width': self.width, 'frame': self.frame, 'regs': regs,
            # pc is ~2 instructions ahead of the accessing instruction (pipeline)
            'pc': regs[15], 'lr': regs[14], 'thumb': bool(self.cpsr & 0x20),
        }


_lib = None


def _load():
    global _lib
    if _lib is None:
        path = HERE / 'libemu.dylib'
        if not path.exists():
            os.system(f'sh {HERE / "build.sh"}')
        _lib = ctypes.CDLL(str(path))
        L = _lib
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
        L.emu_hits.argtypes = [P, ctypes.POINTER(ctypes.POINTER(Hit)), ctypes.POINTER(u32)]
        L.emu_hits.restype = u32
        L.emu_clear_hits.argtypes = [P]
        L.emu_close.argtypes = [P]
        L.emu_set_volume.argtypes = [P, ctypes.c_int]
        L.emu_audio_rate.argtypes = [P]
        L.emu_audio_rate.restype = ctypes.c_uint
        L.emu_audio_clear.argtypes = [P]
        L.emu_run_frames_audio.argtypes = [P, ctypes.c_int, ctypes.POINTER(ctypes.c_int16), u32]
        L.emu_run_frames_audio.restype = u32
    return _lib


class Emu:
    def __init__(self, rom='log2'):
        self.L = _load()
        path = str(ROMS.get(rom, rom))
        self.h = self.L.emu_open(path.encode())
        if not self.h:
            raise RuntimeError(f'cannot open {path}')
        self.keys = 0

    # --- execution / input
    def run(self, frames=1):
        self.L.emu_run_frames(self.h, frames)

    def step(self, n=1):
        for _ in range(n):
            self.L.emu_step(self.h)

    def set_keys(self, keys):
        self.keys = keys
        self.L.emu_set_keys(self.h, keys)

    def press(self, keys, frames=3, after=3):
        self.set_keys(keys)
        self.run(frames)
        self.set_keys(0)
        self.run(after)

    def hold(self, keys, frames):
        self.set_keys(keys)
        self.run(frames)

    @property
    def frame(self):
        return self.L.emu_frame(self.h)

    # --- memory
    def r8(self, a): return self.L.emu_read8(self.h, a)
    def r16(self, a): return self.L.emu_read16(self.h, a)
    def r32(self, a): return self.L.emu_read32(self.h, a)
    def w8(self, a, v): self.L.emu_write8(self.h, a, v & 0xFF)
    def w16(self, a, v): self.L.emu_write16(self.h, a, v & 0xFFFF)
    def w32(self, a, v): self.L.emu_write32(self.h, a, v & 0xFFFFFFFF)

    def read(self, a, n):
        buf = ctypes.create_string_buffer(n)
        self.L.emu_read_block(self.h, a, buf, n)
        return buf.raw

    def ewram(self): return self.read(0x02000000, 0x40000)
    def iwram(self): return self.read(0x03000000, 0x8000)
    def vram(self): return self.read(0x06000000, 0x18000)
    def palram(self): return self.read(0x05000000, 0x400)
    def oam(self): return self.read(0x07000000, 0x400)

    def reg(self, i): return self.L.emu_reg(self.h, i) & 0xFFFFFFFF

    def cycles(self):
        """global CPU cycle count since power-on"""
        self.L.emu_cycles.restype = ctypes.c_uint64
        self.L.emu_cycles.argtypes = [ctypes.c_void_p]
        return self.L.emu_cycles(self.h)
    def set_reg(self, i, v): self.L.emu_set_reg(self.h, i, v)

    # --- video
    def pixels(self):
        """240x160 RGB bytes."""
        p = self.L.emu_video(self.h)
        raw = ctypes.string_at(p, 240 * 160 * 4)
        out = bytearray(240 * 160 * 3)
        out[0::3] = raw[0::4]
        out[1::3] = raw[1::4]
        out[2::3] = raw[2::4]
        return bytes(out)

    def screenshot(self, path, scale=2):
        from PIL import Image
        img = Image.frombytes('RGB', (240, 160), self.pixels())
        if scale != 1:
            img = img.resize((240 * scale, 160 * scale), Image.NEAREST)
        img.save(path)
        return path

    # --- states
    def save(self):
        n = self.L.emu_state_size(self.h)
        buf = ctypes.create_string_buffer(n)
        self.L.emu_save_state(self.h, buf)
        return buf.raw

    def load(self, state):
        self.L.emu_load_state(self.h, state)

    def save_file(self, path): Path(path).write_bytes(self.save())
    def load_file(self, path): self.load(Path(path).read_bytes())

    # --- debugging
    def watch(self, start, end=None, mode='w'):
        """mode: 'w' write, 'r' read, 'rw', 'c' change-only, 'wc' write-that-changes."""
        t = {'w': 1, 'r': 2, 'rw': 3, 'c': 4, 'wc': 5}[mode]
        return self.L.emu_watch(self.h, start, end if end is not None else start + 1, t)

    def breakpoint(self, addr):
        return self.L.emu_break(self.h, addr)

    def clear(self, point_id):
        return self.L.emu_clear(self.h, point_id)

    def hits(self, clear=True):
        ptr = ctypes.POINTER(Hit)()
        dropped = ctypes.c_uint32()
        n = self.L.emu_hits(self.h, ctypes.byref(ptr), ctypes.byref(dropped))
        out = [ptr[i].as_dict() for i in range(n)]
        if clear:
            self.L.emu_clear_hits(self.h)
        return out

    # --- audio (opt-in: the core starts muted)
    def set_volume(self, volume=0x100):
        """0 = mute (default), 0x100 = full GBA mixer output."""
        self.L.emu_set_volume(self.h, volume)

    @property
    def audio_rate(self):
        """Output sample rate in Hz (depends on the game's SOUNDBIAS resolution)."""
        return self.L.emu_audio_rate(self.h)

    def audio_clear(self):
        self.L.emu_audio_clear(self.h)

    def run_audio(self, frames=1):
        """Run frames and return the produced audio as interleaved stereo int16 bytes."""
        cap = frames * 2200 + 4096
        buf = (ctypes.c_int16 * (cap * 2))()
        n = self.L.emu_run_frames_audio(self.h, frames, buf, cap)
        return ctypes.string_at(buf, n * 4)

    def close(self):
        self.L.emu_close(self.h)
