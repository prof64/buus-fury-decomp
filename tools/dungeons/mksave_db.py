"""Dragon Ball dungeon start states (dungeons 1-4) as real save games (the method of tools/superbuu/mksave.py).

usage: .venv/bin/python tools/dungeons/mksave_db.py CHAPTER   -> work/db/<chapter>_start.ss (+ .json / .png / .eeprom)

Base = the ship agent's REAL fat-Buu checkpoint work/ship/fat/d03_hunt.ss (West City after Bulma's Dragon Radar s193:
flags through 77, Trunks active, 5 Hamburgers; every byte of it played by the real game). What a chapter adds on top is
listed in CHAPTERS and is the INVENTED part (a real player would have got it by play): levels (the game's level_up_to
0xF3 + points spent + RecalcStats), the Bandana (the game's list append 0x08004448), earlier Dragon Balls (flags +
supply item, as the pickup events set them), healing supplies, zenie. The save's location is the dungeon entrance's
save point (or arrival spot); the slot is written by the game's writer and loaded through the title screen."""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUT = ROOT / 'work' / 'db'
OUT.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
import mksave  # noqa: E402,F401
from mksave import emu, run_ops, spend_points, title_continue, call_long, sd_read, status, GD, REC  # noqa: E402

BASE = ROOT / 'work' / 'ship' / 'fat' / 'd03_hunt.ss'

# name: dict(loc=(world, area, variant, x, y), char=current character, levels={char: level}, flags=[..],
#            supplies={item: count}, equipment=[ids], zenie=N)
CHAPTERS = {
    # 1: Thieves Den. 9-1's gate wants Trunks at level 80 (s517(3, 80)); the guards want the Bandana (126) worn (the
    # route equips it through the pause menu). Save at 9-1's save point.
    'db1': dict(loc=(9, 1, 1, 61, 196), char=3, levels={3: 90, 2: 80}, flags=[], supplies={3: 10}, equipment=[126],
                zenie=2000),
}


def set_flag(e, f):
    a = GD + 0x55 + (f >> 3)
    e.w8(a, e.r8(a) | (1 << (f & 7)))


def build(name, c=None):
    """c: a chapter dict (default CHAPTERS[name]); optional keys: base (a state path relative to the repo, default
    BASE), ops (extra script natives run through the game's interpreter first: mksave.N names)"""
    c = c or CHAPTERS[name]
    out = OUT / f'{name}_start.ss'
    base = ROOT / c['base'] if c.get('base') else BASE
    e = emu(str(base))
    e.run(2)
    if e.r8(GD) != c['char']:
        run_ops(e, [('set_character', c['char'])])
    if c.get('ops'):
        run_ops(e, c['ops'])
    for ch, lv in c['levels'].items():
        run_ops(e, [('level_up_to', ch, lv)])
        spend_points(e, ch)
    for f in c['flags']:
        set_flag(e, f)
    for it, n in c['supplies'].items():
        e.w8(GD + 2 + it, n)
    for eq in c['equipment']:
        call_long(e, 0x08004448, (GD, eq, 0, 0))
    for ch in range(9):
        r = REC + 0x68 * ch
        e.w16(r + 0x16, e.r16(r + 0x18))
        e.w16(r + 0x1A, e.r16(r + 0x1C))
    if c.get('zenie') is not None:          # None: the base's own zenie
        e.w32(GD + 0x4F4, c['zenie'])
    w, a, v, x, y = c['loc']
    e.w8(GD + 0x8A0, w)
    e.w8(GD + 0x8A1, a)
    e.w8(GD + 0x8A2, v)
    e.w16(GD + 0x8A4, x)
    e.w16(GD + 0x8A6, y)
    # bit 0 = the game exists; bit 1 = the HUD is shown (hide_hud clears it: a base state on the world map has it clear,
    # and a save made there would load without a HUD - a real arrival's script shows it again)
    e.w8(GD + 1, e.r8(GD + 1) | 3)
    call_long(e, 0x080049F2, (GD, 0, 0, 0))
    out.with_suffix('.eeprom').write_bytes(sd_read(e))
    log = []
    title_continue(e, log)
    e.save_file(str(out))
    out.with_suffix('.json').write_text(json.dumps({'chapter': name, **c, 'base': str(base.relative_to(ROOT)),
                                                    'continue_keys': log, 'status': status(e)}))
    print(name, 'loaded', status(e), len(log), 'frames from the soft reset')
    print('equipment list', e.read(GD + 0xE4, 16).hex(), 'supplies', e.read(GD + 2, 25).hex())
    for ch in (2, 3):
        print(ch, e.read(REC + 0x68 * ch, 0x50).hex())
    e.screenshot(str(out.with_suffix('.png')), 1)
    return e


if __name__ == '__main__':
    build(sys.argv[1])
