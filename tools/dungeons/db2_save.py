"""Dragon Ball dungeon 2 (Diablo Desert + Diablo Desert Express, world 11) start state.

usage: .venv/bin/python tools/dungeons/db2_save.py   -> work/db/db2_start.ss

Base = work/db/db1/end.ss (the REAL end of dungeon 1: world map after the Thieves Den, Trunks active level 93, Goten
level 80, flags 80 / 81 / 82 / 112 / 165, the 1-Star Dragon Ball). INVENTED on top (see CHAPTER): the active character
(Goten: 11-1's gate is s517(2, 80, 1), and characters switch only at save points), the save location (11-1, on the
path between the world-map pad and the gate) and the healing supplies."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mksave_db  # noqa: E402

CHAPTER = dict(base='work/db/db1/end.ss', loc=(11, 1, 1, 232, 232), char=2, levels={}, flags=[],
               supplies={}, equipment=[], zenie=2500)   # 9 Hamburgers / 2500 zenie = what the base has

if __name__ == '__main__':
    mksave_db.build('db2', CHAPTER)
