"""DEV start state for the Babidi's ship chapter (route development only, NOT the recording's start):
work/earth/ow_end.ss (the real playthrough at the end of Other World, Gohan on Earth 2-1) + the Earth chapter's
persistent results applied through the game's own natives (story flags 14..53, party joins, level_up_to), then the
real chapter-transition event s314 (title card, warp to Southwest Forest 8-1, Vegeta and Goku join, flag 54).

usage: python3 mkstate_dev.py OUT.ss"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from common import emu, KEY  # noqa: E402
from script_nat_b_lib import run_code, asm  # noqa: E402

ROOT = HERE.parents[2]
BASE = ROOT / 'work/earth/ow_end.ss'
N = lambda i: ('n', i)  # noqa: E731
SET_FLAG, JOIN, LEVEL_TO, RUN_EVENT, SET_CHAR, FLAG2, HEAL = 0x1D, 0x97, 0xF3, 0xB5, 0x95, 0x99, 0xCA


def build(out, levels=None):
    levels = levels or {0: 82, 1: 86, 2: 78, 3: 78, 4: 84}
    e = emu(str(BASE))
    e.run(3)
    items = []
    for f in range(14, 54):
        items += [f, N(SET_FLAG)]
    for c in (2, 3):
        items += [c, N(JOIN), c, N(FLAG2)]
    for c, lv in levels.items():
        items += [c, lv, N(LEVEL_TO)]
    items += [N(HEAL)]
    r = run_code(e, asm(items), max_frames=60)
    print('setup', r)
    r = run_code(e, asm([314, N(RUN_EVENT)]), max_frames=6000, keys=lambda f: KEY.A if f % 30 < 4 else 0)
    print('s314', r)
    e.set_keys(0)
    e.run(30)
    e.save_file(str(out))
    return e


if __name__ == '__main__':
    e = build(Path(sys.argv[1]).resolve())
    sys.path.insert(0, str(HERE))
    from gd import dump
    dump(e)
