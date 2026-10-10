"""Dragon Ball dungeon 3 (Pilaf's Castle, world 10) start state: work/db/db3_start.ss.

Base = work/db/db2/end.ss: the REAL end of dungeon 2 (world map after the Diablo Desert Express, Goten active; itself
played from the real end of dungeon 1), so every story flag / Dragon Ball so far comes from play.
INVENTED on top of it:
  * the save location: 10-1, beside its save point, Trunks (3) active (characters switch at save points)
  * supply 3 (Hamburger) = HAMBURGERS, supply 6 (Turkey) = TURKEYS (dungeon 2 ate the nine Hamburgers the party
    had; the zenie is the base's own)
  * levels: LEVELS (see below; {} = the base's own levels); everyone healed (the save builder's refill)
"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import mksave_db  # noqa: E402

HAMBURGERS = 9
TURKEYS = 4         # with Hamburgers alone the bot dies on the way back (10-14's rooms come back on every pass)
LEVELS = {}
CHAPTER = dict(
    base='work/db/db2/end.ss',
    loc=(10, 1, 1, 190, 140), char=3, levels=LEVELS,
    flags=[], supplies={3: HAMBURGERS, 6: TURKEYS},
    equipment=[], zenie=None,       # the base's own zenie
)

if __name__ == '__main__':
    mksave_db.build('db3', CHAPTER)
