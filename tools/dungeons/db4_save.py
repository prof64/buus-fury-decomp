"""Dragon Ball dungeon 4 (Nataday Village / Northern Wilderness / Volcano, world 15) start state.

usage: .venv/bin/python tools/dungeons/db4_save.py [LEVEL]   -> work/db/db4_start.ss

Base = work/db/db3/end.ss: the REAL end of dungeon 3 (world map after Pilaf's Castle; dungeons 1 .. 3 chained, each
played by the real game from the previous one's end), so flags 112 .. 114 and their Dragon Balls come from play.
INVENTED on top (a real player would have got it by play):
  * flags 116, 117, 118: the Dragon Balls of dungeons 5, 6, 7 (the other dungeon agent's: s387 / s399 / s406), with
    their supply items 30, 31, 32 = 1. The 4-Star ball is the LAST one in the story: the world map shows Nataday's
    radar target only with the six others collected, and the take-off event s515 runs s019 (Goten and Trunks break
    the radar, warp to 15-1) when flags 112 .. 118 == all but 115.
  * flag 107: what s019 sets (the arrival cutscene in 15-1; it makes 15-33's priest scene s437 appear).
  * Goten (character 2) active at level LEVEL (15-1's gate: s517(2, 100, 5)), points spent the way mksave does.
  * supplies: 16 Turkeys (item 6, 800 HP each: with 10 the bot dies on the way back; Hamburgers alone do not
    last to the summit); the base's zenie.
  * location: 15-1 (400, 640), the area's arrival spot below the save point.
Not set (dungeon-internal flags of dungeons 5 .. 7, never read in world 15): 95 .. 106."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mksave_db  # noqa: E402

LEVEL = int(sys.argv[1]) if len(sys.argv) > 1 else 100
CHAPTER = dict(base='work/db/db3/end.ss', loc=(15, 1, 1, 400, 640), char=2, levels={2: LEVEL},
               flags=[116, 117, 118, 107], supplies={6: 16, 30: 1, 31: 1, 32: 1},
               equipment=[], zenie=None)

if __name__ == '__main__':
    mksave_db.build('db4', CHAPTER)
