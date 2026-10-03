"""Build a chapter-start save game in the REAL game and load it through the title screen's Continue.

usage: mksave.py CHAPTER out.ss

Method (all state changes are made by the game's own code):
 1. base = a real playthrough state (work/superbuu/base_ow_end.ss: the story playthrough route played by the real
    game from New Game through Other World to Earth 2-1, chapter "Great Saiyaman Saga", flags 1..13).
 2. the story's state changes between there and the chapter start are applied by running, through the game's own
    script interpreter (0x08004098), the party / form / skill natives the mainline events execute, in story order
    (STORY below, ordered by the story flag each event sets; see tools/superbuu/order.py for the extraction);
    story flags are set as the mainline events set them (FLAGS).
 3. levels: level_up_to (native 0xF3 = the game's level-up routine with its random growth), stat points spent on the
    base stats like the Status screen does, RecalcStats 0x08013C66 per record.
 4. GameData +0x8A0 = the save point location, +1 bit 0 (game exists); written to EEPROM slot 0 by the game's slot
    writer 0x080049F2(gd, slot) (checksum + EEPROM protocol).
 5. soft reset (A+B+START+SELECT) -> title -> Single Player -> Game 1 -> the game loads the slot (Continue).
    Everything in RAM after this point was produced by the game's boot + load path from the EEPROM record.
The output state is taken when the player is controllable in the field after the load (the segment start); the
input program from the soft reset is written next to it (out.json) so the load itself can be replayed too."""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent.parent / 'work' / 'superbuu'   # states, key logs (local)
OUT.mkdir(parents=True, exist_ok=True)
ROOT = HERE.parent.parent
sys.path.insert(0, str(HERE))
from sbemu import sd_read, sd_write  # noqa: E402  (loads the savedata-capable emulator lib first)
sys.path.insert(0, str(ROOT / 're' / 'buu'))
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'menu'))
sys.path.insert(0, str(ROOT / 'tools' / 'story_playthrough'))
from common import emu, KEY  # noqa: E402
from script_nat_b_lib import asm, run_code  # noqa: E402
from flashlib import call_long  # noqa: E402
from route import status  # noqa: E402

GD = 0x03001068
REC = 0x03001560
N = dict(set_flag=29, clear_flag=30, set_character=149, join_party=151, leave_party=152, set_char_flag2=153,
         clear_char_flag2=154, set_char_form=157, learn_skill=164, forget_skill=165, replace_skill=166,
         set_skill_level=233, level_up_to=243)

# (key = story flag position, event, ops) - mainline events after Other World with state changes, in story order.
# Conditional helpers are resolved for the branch the story takes (comments).
STORY = [
    (15, 's124', [('set_character', 1)]),
    (15, 's125', [('forget_skill', 0, 20)]),
    (18, 's128', [('learn_skill', 0, 20)]),
    (25, 's194', [('set_char_form', 1, 111), ('set_char_form', 1, 107)]),
    (26, 's131', [('set_char_form', 1, 111), ('replace_skill', 1, 20, 21)]),
    (34, 's149', [('set_char_form', 1, 107), ('replace_skill', 1, 21, 20)]),
    (36, 's169', [('set_char_form', 1, 111), ('replace_skill', 1, 20, 21)]),
    (38, 's133', [('set_char_form', 1, 107), ('replace_skill', 1, 21, 20)]),
    (43, 's113', [('set_character', 1)]),
    (44, 's191', [('join_party', 3), ('set_character', 3)]),          # Trunks joins (s122 -> s191)
    (44, 's192', [('set_char_form', 3, 269)]),
    (44, 's172', [('set_character', 3)]),
    (45, 's184', [('replace_skill', 1, 18, 20)]),
    (45, 's016', [('set_char_form', 1, 112), ('replace_skill', 3, 20, 21), ('replace_skill', 1, 20, 21)]),
    (48, 's262', [('set_character', 1), ('clear_char_flag2', 3)]),
    (49, 's275', [('set_character', 1), ('set_char_flag2', 1), ('clear_char_flag2', 3)]),
    (50, 's306', [('learn_skill', 8, 7), ('set_char_flag2', 0)]),
    (50, 's260', [('join_party', 0), ('set_char_flag2', 0), ('set_character', 0), ('clear_char_flag2', 1),
                  ('set_char_form', 0, 117)]),
    (54, 's314', [('set_character', 1), ('join_party', 4), ('set_char_flag2', 4), ('set_skill_level', 4, 2, 1),
                  ('join_party', 0), ('set_char_flag2', 0), ('set_char_flag2', 1)]),
    (60, 's327', [('set_character', 4)]),
    (60, 's328', [('set_char_form', 1, 112), ('set_char_form', 4, 279), ('set_character', 0)]),
    (60, 's336', [('set_character', 1)]),
    (60, 's337', [('set_char_form', 1, 106), ('set_char_form', 4, 282), ('clear_char_flag2', 4)]),
    (60, 's339', [('set_char_form', 1, 106), ('set_char_form', 4, 282), ('set_character', 0)]),
    (73, 's341', [('set_character', 4), ('clear_char_flag2', 0), ('clear_char_flag2', 1), ('set_char_flag2', 4),
                  ('replace_skill', 4, 20, 21)]),
    (74, 's342', [('clear_char_flag2', 1)]),
    (75, 's344', [('replace_skill', 4, 21, 20), ('clear_char_flag2', 4)]),
    (75, 's345', [('set_character', 0), ('set_char_flag2', 0)]),
    (76, 's214', [('set_character', 2), ('join_party', 2), ('set_char_flag2', 2), ('set_char_flag2', 3),
                  ('clear_char_flag2', 0), ('clear_char_flag2', 4), ('set_skill_level', 2, 8, 1)]),
    (76, 's219', [('join_party', 2), ('join_party', 3), ('set_char_flag2', 2), ('set_char_flag2', 3)]),
    (119, 's215', [('set_char_form', 3, 269), ('set_char_form', 2, 123), ('set_character', 0)]),
    (119, 's216', [('set_character', 2)]),
    (119, 's217', [('set_character', 0), ('set_char_flag2', 0), ('clear_char_flag2', 2), ('clear_char_flag2', 3)]),
    (120, 's027', [('learn_skill', 0, 22)]),
    # s218: Gotenks (5) fights fat Buu; its defusion s503(4, 3, ..): current char 5 -> leave 5, join 3 / 2, flag2 3 / 2,
    # p0 = 4 -> set_character(3)
    (120, 's218', [('clear_char_flag2', 0), ('set_character', 3), ('learn_skill', 2, 12), ('learn_skill', 3, 12),
                   ('leave_party', 5), ('join_party', 3), ('join_party', 2), ('set_char_flag2', 3),
                   ('set_char_flag2', 2), ('set_character', 3)]),
    # Janemba saga: Goku in Other World; Vegeta joins in HFIL (s446); Janemba (s450 / s451 fusion -> Gogeta 7, s452)
    (121, 's066', [('set_character', 0)]),
    (123, 's446', [('set_char_form', 4, 280), ('set_char_flag2', 4)]),
    (124, 's450', [('set_character', 4), ('set_character', 0)]),
    # s452: s503(1, 5, 102, 32) with current char 7 -> leave 7, join 0 / 4, flag2 4 / 0, p0 = 1 -> set_character(0);
    # then join 0 / 4, flag2 0 / 4, clear flag2 7, leave 7; node 7: clear flag2 4
    (125, 's452', [('leave_party', 7), ('join_party', 0), ('join_party', 4), ('set_char_flag2', 4),
                   ('set_char_flag2', 0), ('set_character', 0), ('join_party', 0), ('join_party', 4),
                   ('set_char_flag2', 0), ('set_char_flag2', 4), ('clear_char_flag2', 7), ('leave_party', 7),
                   ('clear_char_flag2', 4)]),
]

# story flags per chapter start (flags 1..13 and 157 are already set by the real playthrough base)
NOT_SET = {64 + 1, 64 + 2, 75, 95, 122}   # 64/65/66: one switch of three (64 kept); 75 / 95: set and cleared again
# area puzzle / gate flags the mainline sets on the way (HFIL, Janemba saga): 18-2 switch, 18-12 Vegeta Lv100 gate,
# 18-10 floor puzzle solved
EXTRA_SET = {125: {181, 183, 284}}
CHAPTERS = {
    # name: (last story flag, location (world, area, variant, x, y) = a save point, levels per char)
    'superbuu': (125, (18, 13, 1, 127, 163), {0: 180, 4: 180, 1: 160, 2: 165, 3: 165}),
}


ZENIE = 18000   # zenie a player carries at this point (coins are collected by walking over them)


def run_ops(e, ops):
    items = []
    for op in ops:
        items += list(op[1:]) + [('n', N[op[0]])]
    r = run_code(e, asm(items), max_frames=300)
    assert r[0] is not None, ('script did not finish', ops)
    e.run(2)


def set_flags(e, upto):
    fl = bytearray(e.read(GD + 0x55, 143))
    for f in range(1, upto + 1):
        if f not in NOT_SET:
            fl[f >> 3] |= 1 << (f & 7)
    for k, fs in EXTRA_SET.items():
        if k <= upto:
            for f in fs:
                fl[f >> 3] |= 1 << (f & 7)
    for i, b in enumerate(fl):
        e.w8(GD + 0x55 + i, b)


def spend_points(e, c):
    """the Status screen's allocation: 35 % STR, 25 % POW, 25 % END, rest SPD (1:1 into the base stats)"""
    r = REC + 0x68 * c
    pts = e.r8(r + 0x2C)
    a = [pts * 35 // 100, pts * 25 // 100, pts * 25 // 100]
    a.append(pts - sum(a))
    for i in range(4):
        e.w8(r + 0x28 + i, min(255, e.r8(r + 0x28 + i) + a[i]))
    e.w8(r + 0x2C, 0)
    call_long(e, 0x08013C66, (r, 0, 0, 0))


def title_continue(e, log):
    """soft reset -> title -> Single Player -> Select Game: Game 1 -> A; returns when the field is controllable"""
    def k(keys, n):
        for _ in range(n):
            e.set_keys(keys)
            e.run(1)
            log.append(keys)
    k(KEY.A | KEY.B | KEY.START | KEY.SELECT, 10)
    k(0, 5)
    # logos / intro: START every 40 frames until the title menu; then A on Single Player, A on Game 1
    for i in range(30):
        k(KEY.START, 4)
        k(0, 36)
        if status(e)['p'] is not None:
            break
    for i in range(400):
        st = status(e)
        if st['p'] is not None and st['scene'] == 0:
            break
        k(KEY.A if i % 20 < 3 else 0, 1)
    k(0, 30)


def build(chapter, out):
    flag, loc, levels = CHAPTERS[chapter]
    e = emu(str(OUT / 'base_ow_end.ss'))
    e.run(2)
    for key, ev, ops in STORY:
        if key <= flag:
            run_ops(e, ops)
    set_flags(e, flag)
    for c, lv in levels.items():
        run_ops(e, [('level_up_to', c, lv)])
        spend_points(e, c)
    # heal everyone (a save point heals) and write the location + "game exists" bit, then the slot
    for c in range(9):
        r = REC + 0x68 * c
        e.w16(r + 0x16, e.r16(r + 0x18))
        e.w16(r + 0x1A, e.r16(r + 0x1C))
    e.w32(GD + 0x4F4, ZENIE)
    w, a, v, x, y = loc
    e.w8(GD + 0x8A0, w); e.w8(GD + 0x8A1, a); e.w8(GD + 0x8A2, v)
    e.w16(GD + 0x8A4, x); e.w16(GD + 0x8A6, y)
    e.w8(GD + 1, e.r8(GD + 1) | 1)
    gd = e.read(GD, 0x8A8)
    call_long(e, 0x080049F2, (GD, 0, 0, 0))
    eeprom = sd_read(e)
    (Path(out).with_suffix('.eeprom')).write_bytes(eeprom)
    (Path(out).with_suffix('.gd')).write_bytes(gd)
    log = []
    title_continue(e, log)
    e.save_file(out)
    Path(out).with_suffix('.json').write_text(json.dumps({
        'chapter': chapter, 'last_flag': flag, 'location': loc, 'levels': levels, 'base': 'base_ow_end.ss',
        'continue_keys': log, 'status': status(e)}))
    print(chapter, 'loaded', status(e), len(log), 'frames from the soft reset')
    return e


if __name__ == '__main__':
    e = build(sys.argv[1], sys.argv[2])
    e.screenshot(str(Path(sys.argv[2]).with_suffix('.png')), 1)
