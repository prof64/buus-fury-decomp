"""The story playthrough's input program and emulator helpers (headless, muted).

The route starts at a soft reset from work/states/buu_menus.ss (exactly like the intro cutscene recording,
tools/script_lockstep/full_scenarios.py) and plays the opening of the campaign with a fixed key program."""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 're' / 'buu'))
sys.path.insert(0, str(ROOT / 'tools' / 'script_lockstep'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from common import emu, KEY  # noqa: E402

NAMES = {'A': KEY.A, 'B': KEY.B, 'L': KEY.LEFT, 'R': KEY.RIGHT, 'U': KEY.UP, 'D': KEY.DOWN, 'S': KEY.START,
         'E': KEY.SELECT, 'LS': KEY.L, 'RS': KEY.R, '0': 0}

WS, PLAYER, GD, FC = 0x0300197C, 0x03001C34, 0x03001068, 0x03000F94
DEPTH = 0x03002A5C          # run_event nesting (scene depth)


def soft_reset(e):
    e.set_keys(KEY.A | KEY.B | KEY.START | KEY.SELECT)
    e.run(10)
    e.set_keys(0)
    e.run(5)


def start_emu(state):
    """'reset' = buu_menus.ss + settle 3 + soft reset (the intro recording's start); else a save state file"""
    if state == 'reset':
        e = emu('buu_menus.ss')
        e.run(3)
        soft_reset(e)
        return e
    return emu(state)


def keys_of(s):
    k = 0
    for n in s.split('+'):
        k |= NAMES[n]
    return k


def program_keys(prog):
    """'R:120,0:30:a,U+R:20' -> per-frame keys; ':a' ORs in the A mash (4 of every 30 frames, phase from the
    segment start)"""
    out = []
    for seg in prog.split(','):
        seg = seg.strip()
        if not seg:
            continue
        parts = seg.split(':')
        k, n = keys_of(parts[0]), int(parts[1])
        mash = len(parts) > 2 and parts[2] == 'a'
        for g in range(n):
            out.append(k | (KEY.A if mash and g % 30 < 4 else 0))
    return out


def s32v(v):
    return v - (1 << 32) if v & 0x80000000 else v


def status(e):
    p = e.r32(PLAYER)
    pl = None
    if p:
        pl = [s32v(e.r32(p + 0x1A0)) >> 8, s32v(e.r32(p + 0x1A4)) >> 8, e.r8(p + 0x10)]
    rec = 0x03001560 + 0x68 * e.r8(GD)
    return {'area': [e.r8(WS), e.r8(WS + 1), e.r8(WS + 2)], 'p': pl, 'scene': e.r32(DEPTH), 'lf': e.r32(FC),
            'hp': e.r16(rec + 0x16), 'zenie': e.r32(GD + 0x4F4)}


# --- the route ---------------------------------------------------------------------------------------------------

INTRO_START = 847   # frame (after the soft reset) at which New Game runs scene #4


def intro_keys(f):
    """title -> New Game: alternate START / A every 40 frames; then A 4 of every 30 frames (text boxes)"""
    if f < INTRO_START - 20:
        if f % 40 < 4:
            return KEY.START if (f // 40) % 2 == 0 else KEY.A
        return 0
    if f < INTRO_START:
        return 0
    g = f - INTRO_START
    return KEY.A if g % 30 < 4 else 0


INTRO_FRAMES = 6800     # the prologue (#4 -> #38 -> #516 -> warp to Snakeway -> walks) is over by then


# after the intro (Goku controllable on Snakeway 1-1): driver.py goals, played from frame INTRO_FRAMES
ROUTE_GOALS = [
    # Snakeway 1-1: east along the road (the s039 "run" hint fires on the way), out by the right edge
    ('walk', 944, 170), ('hold', KEY.RIGHT, 20), ('area', 1, 2), ('wait', 40),
    # Snakeway 1-2: to the top exit -> 1-5 (the steps up to Yemma's office) -> 1-3
    ('walk', 336, 24), ('hold', KEY.UP, 30), ('area', 1, 5), ('wait', 60),
    ('walk', 336, 240), ('walk', 336, 170), ('hold', KEY.UP, 30), ('area', 1, 3), ('wait', 60),
    # Yemma's office: A at the counter (event s048: King Kai joins, "the door to your left")
    ('walk', 232, 362), ('talk', 'U'), ('wait', 30),
    # the ogre in the left doorway steps aside once spoken to (s055), then the door -> 1-4; its trigger s061 (the
    # Higher Plane) plays a scene and warps to Grand Kai's Mansion 1-10
    ('walk', 112, 256), ('talk', 'U'), ('walk', 110, 216), ('hold', KEY.UP, 30), ('area', 1, 4), ('wait', 60),
    ('walk', 330, 290), ('area', 1, 10), ('events',), ('wait', 30),
    # Grand Kai's Mansion 1-10 -> down -> Grand Kai's Planet 1-11 (Olibu meets Goku, s073) -> east -> 1-12, the
    # first fighters (s086 "You're going down rookie!": the two NPCs become enemies)
    ('walk', 272, 392), ('hold', KEY.DOWN, 30), ('area', 1, 11), ('expect', 200), ('wait', 30),
    ('walk', 584, 180), ('hold', KEY.RIGHT, 30), ('area', 1, 12), ('expect', 200), ('wait', 30),
    # the first fight: punch both fighters (field enemies, type 86) until they are down
    ('fight', 3000), ('wait', 30),
    # the Battlefield Royale: 1-12 up -> 1-13 (s087, three fighters; Goku levels up) -> down-right -> 1-14 (s088, four
    # fighters) -> right -> 1-15: s089 and the first boss (sprite 206, a script-created enemy with a boss bar), its
    # defeat scene, the cave door
    ('walk', 244, 12), ('hold', KEY.UP, 30), ('area', 1, 13), ('wait', 30), ('walk', 76, 206), ('events',),
    ('fight', 4000), ('wait', 30),
    ('walk', 290, 240), ('hold', KEY.DOWN, 30), ('area', 1, 14), ('wait', 30), ('walk', 40, 30), ('events',),
    ('fight', 6000), ('wait', 30),
    ('walk', 296, 220), ('hold', KEY.RIGHT, 30), ('area', 1, 15), ('expect', 400), ('fight', 12000), ('wait', 60),
    ('events',), ('wait', 30),
    # --- part 2 (recording extended 2026-10-02) ---
    # the Character Gate to the cave (1-15, level 65 for Goku): grind the fighters, which come back on every load
    # of 1-14 (4) and 1-13 (3): 15 -> 14 -> 13 -> 14 -> 13 -> 14 -> 15
    ('walk', 12, 116), ('hold', KEY.LEFT, 30), ('area', 1, 14), ('wait', 30), ('fight', 6000), ('wait', 30),
    ('walk', 40, 10), ('hold', KEY.UP, 30), ('area', 1, 13), ('wait', 30), ('fight', 6000), ('wait', 30),
    ('walk', 290, 240), ('hold', KEY.DOWN, 30), ('area', 1, 14), ('wait', 30), ('fight', 6000), ('wait', 30),
    ('walk', 40, 10), ('hold', KEY.UP, 30), ('area', 1, 13), ('wait', 30), ('fight', 6000), ('wait', 30),
    ('walk', 290, 240), ('hold', KEY.DOWN, 30), ('area', 1, 14), ('wait', 30), ('fight', 6000), ('wait', 30),
    ('walk', 296, 220), ('hold', KEY.RIGHT, 30), ('area', 1, 15), ('wait', 60),
    # the gate (s091 explains it on the way; A facing up opens it at level 65), the Cave of the Ancients 1-16
    ('walk', 256, 134), ('talk', 'U'), ('wait', 30), ('hold', KEY.UP, 60), ('area', 1, 16), ('events',), ('wait', 30),
    # 1-16: the door (s094: Kamehameha tutorial): face up, L to the Kamehameha icon, hold B; then up to 1-17 (s096)
    ('walk', 112, 140), ('hold', KEY.UP, 2), ('wait', 5), ('hold', KEY.L, 2), ('wait', 10), ('hold', KEY.B, 120),
    ('wait', 40), ('events',), ('walk', 112, 124), ('hold', KEY.UP, 40), ('area', 1, 17), ('expect', 200),
    ('wait', 60),
    # --- part 3 ---
    # 1-17: the Katchin wall. L to Instant Transmission (slot 2), hold B (aim cursor), steer it right then down onto
    # free floor beside the chest (a blocked spot does nothing), release: Goku lands at (200, 228) inside s099 (the
    # chest), then teleports back; s100 ("Amazing, Goku!") warps to Grand Kai's Mansion 1-10 (a long scene)
    ('walk', 112, 212), ('hold', KEY.L, 2), ('wait', 10), ('hold', KEY.B, 20), ('hold', KEY.B | KEY.RIGHT, 23),
    ('hold', KEY.B | KEY.DOWN, 12), ('expect', 60), ('wait', 20), ('expect', 200), ('wait', 30),
    # 1-10: up into s101 (the tournament, flag 12) -> 1-20, the bracket scene; the final: Pikkon (a field enemy);
    # his defeat scene -> 0-22 / 0-96 cutscenes -> Earth 2-1 (end of Other World)
    ('walk', 272, 172), ('hold', KEY.UP, 10), ('expect', 100), ('wait', 30), ('fight', 8000), ('wait', 30),
    ('events',), ('wait', 60),
]
