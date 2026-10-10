"""Reproducible emulator states built from the ROMs alone (cached under work/gfx/states/)."""
import sys
import struct
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'emu'))
from emu import Emu, KEY  # noqa: E402

CACHE = ROOT / 'work' / 'gfx' / 'states'


def log2_debug_menu():
    """Boot LoG2, reach the title screen and switch it to the hidden Debug menu
    (title object +8 = 2, same trick as re/log2/debugmenu.py)."""
    p = CACHE / 'log2_debug.ss'
    e = Emu('log2')
    if p.exists():
        e.load_file(p)
        return e
    e.run(600)
    bp = e.breakpoint(0x08000434)
    obj = None
    for _ in range(40):
        e.press(KEY.START, 3, 40)
        h = e.hits()
        if h:
            obj = h[-1]['regs'][0]
            break
    e.clear(bp)
    e.run(30)
    assert obj, 'title object not found'
    e.w8(obj + 8, 2)
    e.press(KEY.A, 3, 60)
    e.run(60)
    CACHE.mkdir(parents=True, exist_ok=True)
    e.save_file(p)
    return e


def log2_warp(map_index, variation=0, frames=260):
    """Load a map through the debug menu's MAP TEST (menu object found by its vtable 0x08023EE8)."""
    e = log2_debug_menu()
    d = e.ewram()
    i = d.find(struct.pack('<I', 0x8023EE8))
    while i != -1 and i % 4:
        i = d.find(struct.pack('<I', 0x8023EE8), i + 1)
    m = 0x02000000 + i
    e.w32(m + 8, 2)
    e.w32(m + 0x14, map_index)
    e.w32(m + 0x18, variation)
    e.press(KEY.A, 3, 3)
    e.run(frames)
    return e


# ---------------------------------------------------------------- Buu's Fury
BUU_PLAYER = 0x03001C34
BUU_AREA = 0x03001980
BUU_TRIG = 0x0203DFA4      # 'too far to walk' text trigger in the first Snakeway area
BUU_EXIT = 0x0203FF40      # scratch RAM for exit data


def buu_snakeway():
    """Boot Buu's Fury from the ROM to the first controllable Snakeway area."""
    p = CACHE / 'buu_snakeway.ss'
    e = Emu('buu')
    if p.exists():
        e.load_file(p)
        return e
    e.run(300)
    for i in range(600):
        e.press(KEY.START if i % 3 == 0 else KEY.A, 3, 27)
        if e.r32(BUU_TRIG) == 0x08049BD4 and e.r32(BUU_PLAYER):
            break
    for i in range(300):         # clear the opening dialogue until the player can walk
        e.press(KEY.A, 3, 27)
        if i >= 120 and i % 5 == 4:
            s = e.save()
            pl = e.r32(BUU_PLAYER)
            x0 = e.r32(pl + 0x1A0)
            e.hold(KEY.LEFT, 12)
            e.set_keys(0)
            moved = e.r32(pl + 0x1A0) != x0
            e.load(s)
            if moved:
                break
    e.run(60)
    CACHE.mkdir(parents=True, exist_ok=True)
    e.save_file(p)
    return e


def buu_warp(world, area, x=0x100, y=0x100, frames=400):
    """Teleport by converting the Snakeway text trigger into an area-exit trigger
    (technique from re/buu/warp.py: exit data +0x10 facing, +0x11 world, +0x12 map, +0x14/16 dst x/y)."""
    e = buu_snakeway()
    assert e.r32(BUU_TRIG) == 0x08049BD4
    for i in range(0, 0x20, 4):
        e.w32(BUU_EXIT + i, 0)
    e.w8(BUU_EXIT + 0x10, 2)
    e.w8(BUU_EXIT + 0x11, world)
    e.w8(BUU_EXIT + 0x12, area)
    e.w16(BUU_EXIT + 0x14, x)
    e.w16(BUU_EXIT + 0x16, y)
    e.w32(BUU_TRIG, 0x08049BC4)
    e.w8(BUU_TRIG + 0x10, 2)
    e.w8(BUU_TRIG + 0x11, 0xFF)
    e.w32(BUU_TRIG + 0x14, BUU_EXIT)
    p = e.r32(BUU_PLAYER)
    e.w32(p + 0x1A0, 0x160 << 8)
    e.w32(p + 0x1A4, 0xA0 << 8)
    area0 = e.r32(BUU_AREA)
    e.set_keys(KEY.RIGHT)
    for _ in range(frames):
        p = e.r32(BUU_PLAYER)
        if e.r32(BUU_AREA) == area0 and p:
            e.w16(BUU_EXIT + 8, e.r32(p + 0x1A0) >> 8)
            e.w16(BUU_EXIT + 0xA, e.r32(p + 0x1A4) >> 8)
        e.run(1)
        if e.r32(BUU_AREA) != area0:
            e.set_keys(0)
    e.set_keys(0)
    e.run(90)
    return e, e.r32(BUU_AREA) != area0


# ---------------------------------------------------------------- LoG1
LOG1_LOAD_MAP = 0x08001FA8     # load_map(map_index, entrance_ptr)  (from re/log1/warp.py)
LOG1_MAP_TABLE = 0x081CA09C


def _log1_stop_at_loadmap(e, budget=3000):
    """Run (pressing A/START alternately) until the next load_map call; leave the CPU at its entry."""
    bp = e.breakpoint(LOG1_LOAD_MAP)
    for f in range(budget):
        st = e.save()
        k = (KEY.START if (f // 8) % 4 == 0 else KEY.A) if (f // 4) % 2 == 0 else 0
        e.set_keys(k)
        e.run(1)
        if e.hits():
            e.clear(bp)
            e.load(st)
            e.set_keys(k)
            for _ in range(3_000_000):
                e.step()
                if e.reg(15) == LOG1_LOAD_MAP + 4 and (e.reg(16) & 0x20):
                    return True
            return False
    e.clear(bp)
    return False


def log1_preload():
    """State a few frames before the first load_map call after boot (cached)."""
    p = CACHE / 'log1_preload.ss'
    e = Emu('log1')
    if p.exists():
        e.load_file(p)
        return e
    e.run(300)
    bp = e.breakpoint(LOG1_LOAD_MAP)
    snaps = []
    for f in range(20000):
        snaps.append(e.save())
        snaps = snaps[-4:]
        k = (KEY.START if (f // 8) % 4 == 0 else KEY.A) if (f // 4) % 2 == 0 else 0
        e.set_keys(k)
        e.run(1)
        if e.hits():
            break
    e.clear(bp)
    e.load(snaps[0])
    CACHE.mkdir(parents=True, exist_ok=True)
    e.save_file(p)
    return e


def log1_warp(map_index, frames=150):
    e = log1_preload()
    if not _log1_stop_at_loadmap(e, 600):
        raise RuntimeError('load_map not reached')
    rec = e.r32(LOG1_MAP_TABLE + 4 * map_index)
    e.set_reg(0, map_index)
    e.set_reg(1, e.r32(rec + 0x14) + 0x28)
    e.set_keys(0)
    e.run(frames)
    return e
