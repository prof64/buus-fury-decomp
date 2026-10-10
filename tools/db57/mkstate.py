"""Start states for the Dragon Ball dungeons 5-7 (Airship 12, Ninja Stronghold 13, Kyodai Pyramid 14) and the events
after the seventh ball, as save games loaded by the REAL game (the Super Buu agent's method, tools/superbuu/mksave.py).

usage: .venv/bin/python tools/db57/mkstate.py CHAPTER [out.ss]      (default out: work/db57/<chapter>_start.ss)

Base = work/ship/fat/d02_radar.ss: the ship agent's REAL continuous route right after Bulma hands over the Dragon
Radar (s193, story flags through 77; Goten 75 / Trunks 65 as played). On top of it, INVENTED (documented in
engine/docs/segments.md §10):
  * story flags of the balls a player would already hold (FLAGS per chapter) + their Dragon Ball items;
  * levels: Goten and Trunks through the game's own level_up_to native (random growth), points spent like the Status
    screen (mksave.spend_points), full HP / EP;
  * supplies (8 Steaks, 3 Senzu) and zenie;
  * the save location = the dungeon entrance's own save point.
Everything is then written by the game's slot writer and loaded through the title screen (Continue), so every RAM
byte of the start state comes from the game's boot + load path."""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
import mksave  # noqa: E402  (loads the savedata-capable emulator first)
from mksave import GD, REC, run_ops, spend_points, title_continue, sd_read, call_long, emu, status  # noqa: E402

OUT = ROOT / 'work' / 'db57'
BASE = ROOT / 'work' / 'ship' / 'fat' / 'd02_radar.ss'

# supplies: 7 Steak (600 HP, 410 z at the Z-Mart), 21 Senzu Bean; 26 .. 32 = Dragon Ball 1 .. 7
BALL_ITEM = {112: 26, 113: 27, 114: 28, 115: 29, 116: 30, 117: 31, 118: 32}

CHAPTERS = {
    # name: (flags set on top of the base, location (world, area, variant, x, y), current char, levels, supplies)
    # Airship: the balls of Thieves Den / Diablo Desert / Pilaf's Castle held
    'db5': ({112, 113, 114}, (12, 1, 1, 252, 144), 3, {2: 100, 3: 100}, {7: 8, 21: 3}),
    # Ninja Stronghold (Goten opens the 13-1 gate): + the Airship's ball
    'db6': ({112, 113, 114, 116}, (13, 1, 1, 188, 252), 2, {2: 100, 3: 100}, {7: 8, 21: 3}),
    # Kyodai Pyramid (Trunks opens the 14-1 gate): + the Ninja's ball (96 .. 99: the stronghold's own flags)
    'db7': ({112, 113, 114, 116, 117, 96, 97, 98, 99}, (14, 1, 1, 92, 262), 3, {2: 100, 3: 100}, {7: 8, 21: 3}),
    # Goku 100 (the real route left him at 70 with 39 unspent points): at 70 he needs four trips to the pause menu
    # against Majin Buu's 45,000 HP in 0-21, and a pause menu ends a segment's exact match (the menu redraw timing is
    # not modelled exactly) - there is no area-entry cut between that fight and Gotenks vs Buu
    # after the seventh ball (Nataday Village's save point; Broly s436 111, the 4-Star ball s439 115; the dungeons'
    # own flags 96 .. 106): the world map's airship marker starts s017 (the summoning)
    'dbx': ({107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105,
             106}, (15, 1, 1, 420, 636), 2, {2: 110, 3: 110, 0: 100}, {7: 8, 21: 3}),
}
ZENIE = 3000


def build(chapter, out):
    flags, loc, cur, levels, supplies = CHAPTERS[chapter]
    e = emu(str(BASE))
    e.run(2)
    fl = bytearray(e.read(GD + 0x55, 143))
    # flag 77 (s193: Bulma handed over the Dragon Radar - the world map shows the radar with it): the base state
    # turned out not to have it yet
    for f in flags | {77}:
        fl[f >> 3] |= 1 << (f & 7)
        if f in BALL_ITEM:
            e.w8(GD + 2 + BALL_ITEM[f], 1)
    for i, b in enumerate(fl):
        e.w8(GD + 0x55 + i, b)
    for c, lv in levels.items():
        run_ops(e, [('level_up_to', c, lv)])
        spend_points(e, c)
    run_ops(e, [('set_character', cur)])
    for c in range(9):
        r = REC + 0x68 * c
        e.w16(r + 0x16, e.r16(r + 0x18))
        e.w16(r + 0x1A, e.r16(r + 0x1C))
    for it, n in supplies.items():
        e.w8(GD + 2 + it, n)
    e.w32(GD + 0x4F4, ZENIE)
    w, a, v, x, y = loc
    e.w8(GD + 0x8A0, w); e.w8(GD + 0x8A1, a); e.w8(GD + 0x8A2, v)
    e.w16(GD + 0x8A4, x); e.w16(GD + 0x8A6, y)
    e.w8(GD + 1, e.r8(GD + 1) | 1)
    call_long(e, 0x080049F2, (GD, 0, 0, 0))
    Path(out).with_suffix('.eeprom').write_bytes(sd_read(e))
    log = []
    title_continue(e, log)
    e.save_file(str(out))
    Path(out).with_suffix('.json').write_text(json.dumps({
        'chapter': chapter, 'flags': sorted(flags), 'location': loc, 'levels': levels, 'supplies': supplies,
        'base': str(BASE.relative_to(ROOT)), 'continue_keys': log, 'status': status(e)}))
    print(chapter, 'loaded', status(e), len(log), 'frames from the soft reset')
    return e


if __name__ == '__main__':
    ch = sys.argv[1]
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else OUT / f'{ch}_start.ss'
    out.parent.mkdir(parents=True, exist_ok=True)
    e = build(ch, out)
    e.screenshot(str(out.with_suffix('.png')), 1)
