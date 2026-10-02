"""Buu's Fury (Webfoot) sound driver: addresses, data tables, and a frozen-game emulator harness that plays
songs / sound effects through the game's own driver and captures the mixed output.

See tools/audio/AUDIO.md for how the driver works and how these addresses were found.
"""
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
sys.path.insert(0, str(ROOT / 'tools' / 'extract'))
from emu import Emu  # noqa: E402

ROM = ROOT / 'roms' / 'buu.gba'

# ---- driver (all VERIFIED in the emulator, see AUDIO.md)
SOUND = 0x03003898            # sound system object ([0x08072824+4]); +0xC music player, +0x5C4 SFX voices
PLAYER = SOUND + 0x0C         # music player: +0 speed, +1 row, +2 rows, +5 playing, +6 samples/tick, +0xC song hdr, +0x10 order
VOICES = SOUND + 0x5C4        # 16 SFX voices x 0x40: +0x10 flags (bit0 busy), +0x12 vol, +0x13 pan, +0x28 entry, +0x30 pitch
N_VOICES = 16
SONG_TABLE = 0x083BB78C       # 53 x 20 bytes: patterns*, orders*, instruments*, samples*, tempo u8, channels u8
N_SONGS = 53
SFX_TABLE = 0x083192A8        # 64 x 12 bytes: pcm* (s8), loop u8 / loop start u16 @+6, length u16 @+8, rate u16 @+10
N_SFX = 64
PLAY_SONG = 0x0803F110        # PlaySong(player, songHeader)
CLEAR_TRACKS = 0x0803F0E6     # silence all 15 tracks of a player
PLAY_SFX = 0x0803E378         # PlaySfx(voices, entry, volume 0..0x40, pan s8): first free voice, else dropped
STOP_ALL_SFX = 0x0803E2EC     # StopAllSfx(voices)
STATE_UPDATE_CALL = 0x0803D80A  # main loop: BL to the game-state update (virtual call); sound update follows
MIX_RATE = 16777216 / 1050    # timer 0 reload 0xFBE6 -> 15978 Hz stereo (DMA A = left, DMA B = right)

# Thumb stub placed in ROM zero padding: runs one queued call per frame instead of the game-state update,
# so the game logic is frozen while the main loop keeps running the sound driver.
STUB = 0x082ED300
MAILBOX = 0x0203FFD0          # +0 func|1 (0 = idle), +4..+0x10 r0..r3, +0x14 result
_STUB_CODE = [0xB590, 0x4F07, 0x683C, 0x2C00, 0xD008, 0x6878, 0x68B9, 0x68FA, 0x693B, 0xF000, 0xF804,
              0x6178, 0x2100, 0x6039, 0xBD90, 0x4720]


def _rom():
    return ROM.read_bytes()


def song_entry(rom, i):
    a = SONG_TABLE - 0x08000000 + 20 * i
    pat, orders, inst, smp = struct.unpack_from('<4I', rom, a)
    tempo, channels = rom[a + 16], rom[a + 17]
    o = orders - 0x08000000
    order_list = []
    while rom[o] != 0xFF and len(order_list) < 256:
        order_list.append(rom[o])
        o += 1
    return {'header': SONG_TABLE + 20 * i, 'patterns': pat, 'orders': orders, 'instruments': inst,
            'samples': smp, 'tempo': tempo, 'channels': channels, 'order_list': order_list}


def sfx_entry(rom, i):
    a = SFX_TABLE - 0x08000000 + 12 * i
    pcm, w1, w2 = struct.unpack_from('<3I', rom, a)
    return {'entry': SFX_TABLE + 12 * i, 'pcm': pcm, 'loop': bool(w1 & 0xFF), 'loop_start': w1 >> 16,
            'length': w2 & 0xFFFF, 'rate': w2 >> 16}


def _bl(src, tgt):
    off = tgt - (src + 4)
    return 0xF000 | ((off >> 12) & 0x7FF), 0xF800 | ((off >> 1) & 0x7FF)


class Harness:
    """Emulator with the game frozen (state update replaced by the call stub) and audio capture on."""

    def __init__(self, state_emu=None):
        if state_emu is None:
            import emu_states
            state_emu = emu_states.buu_snakeway()
        self.e = e = state_emu
        for i, h in enumerate(_STUB_CODE):
            e.w16(STUB + 2 * i, h)
        e.w32(STUB + 0x20, MAILBOX)
        for i in range(6):
            e.w32(MAILBOX + 4 * i, 0)
        hi, lo = _bl(STATE_UPDATE_CALL, STUB)
        e.w16(STATE_UPDATE_CALL, hi)
        e.w16(STATE_UPDATE_CALL + 2, lo)
        e.set_keys(0)
        e.set_volume(0x100)
        self.silence()

    @property
    def rate(self):
        return self.e.audio_rate

    def call(self, func, *args):
        e = self.e
        for i in range(4):
            e.w32(MAILBOX + 4 + 4 * i, args[i] if i < len(args) else 0)
        e.w32(MAILBOX, func | 1)
        pcm = e.run_audio(1)
        assert e.r32(MAILBOX) == 0, 'stub did not run'
        return e.r32(MAILBOX + 0x14), pcm

    def silence(self, frames=8):
        e = self.e
        e.w8(PLAYER + 5, 0)
        self.call(CLEAR_TRACKS, PLAYER)
        self.call(STOP_ALL_SFX, VOICES)
        e.run_audio(frames)
        e.audio_clear()

    def busy_voices(self):
        return sum(self.e.r8(VOICES + 0x40 * v + 0x10) & 1 for v in range(N_VOICES))
