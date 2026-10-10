"""Side-content start states as real save games (the Super Buu agent's method, tools/superbuu/mksave.py, reused
unchanged): extra chapters registered here, then mksave.build.

usage: python tools/side/mksave_side.py CHAPTER work/side/OUT.ss

Chapters:
  janemba_arena  the save point 18-13 (flags through 123 + 181 / 183 / 284): s448 -> 18-14 Janemba -> Super Janemba
  janemba  the Janemba saga (HFIL): story flags through 120 (fat Buu, s027 / s218); the save's location is 1-5's
           arrival point, where s066 (flag 115 set, 121 clear) starts the saga on arrival (it switches to Goku);
           the 1-5 Instant Transmission pad (flag 121) -> travel map 0-51 -> HFIL 18-1 ... Janemba 18-13."""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / 'tools' / 'superbuu'))
import mksave  # noqa: E402

# name: (last story flag, location (world, area, variant, x, y), levels per character)
SIDE = {
    'janemba': (120, (1, 5, 1, 344, 264), {0: 140, 4: 100, 1: 110, 2: 110, 3: 110}),
    # the save point next to Janemba's arena (18-13), after Vegeta joined (s446, flag 123) and the HFIL puzzle /
    # gate flags (18-2 switch 181, 18-12 Vegeta Lv 100 gate 183, 18-10 floor puzzle 284): s448 -> Janemba
    'janemba_arena': (123, (18, 13, 1, 127, 163), {0: 200, 4: 200, 1: 110, 2: 110, 3: 110}),
}
mksave.EXTRA_SET.setdefault(123, set()).update({181, 183, 284})

# after the story ops: the player switches to Goku (the status menu's character switch) before saving - the Janemba
# saga is Goku's (Other World; the Instant Transmission pads are his)
# ... and carries what a player has by then (bought with the zenie: the food stalls sell these): 3 Senzu Beans,
# 4 Three Course Meals, 5 Curry Plates (native 0x81 add_item: the supply counts, silently)
mksave.N['add_item'] = 0x81
EXTRA_OPS = {'janemba': [('set_character', 0), ('add_item', 21, 3), ('add_item', 8, 4), ('add_item', 5, 5)],
             'janemba_arena': [('set_character', 0)]}

if __name__ == '__main__':
    mksave.CHAPTERS.update(SIDE)
    for ch, ops in EXTRA_OPS.items():
        if ch == sys.argv[1]:
            mksave.STORY.append((SIDE[ch][0], 'side', ops))
    e = mksave.build(sys.argv[1], sys.argv[2])
    e.screenshot(str(Path(sys.argv[2]).with_suffix('.png')), 1)
