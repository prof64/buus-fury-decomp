"""Side-content start states as real save games (the Super Buu agent's method, tools/superbuu/mksave.py, reused
unchanged): extra chapters registered here, then mksave.build.

usage: python tools/side/mksave_side.py CHAPTER work/side/OUT.ss

Chapters:
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
    'janemba': (120, (1, 5, 1, 344, 264), {0: 120, 4: 100, 1: 110, 2: 110, 3: 110}),
}

if __name__ == '__main__':
    mksave.CHAPTERS.update(SIDE)
    e = mksave.build(sys.argv[1], sys.argv[2])
    e.screenshot(str(Path(sys.argv[2]).with_suffix('.png')), 1)
