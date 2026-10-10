"""Test state for the Broly fight only (NOT a recording start): db4's save moved to the Volcano's top ledge 15-19
with the gate / bridge flags set. usage: db4_testsave.py LEVEL -> work/db/db4t<LEVEL>_start.ss"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mksave_db  # noqa: E402

LEVEL = int(sys.argv[1])
mksave_db.build(f'db4t{LEVEL}', dict(
    base='work/db/db1/end.ss', loc=(15, 19, 1, 224, 28), char=2, levels={2: LEVEL},
    flags=[113, 114, 116, 117, 118, 107, 108, 109, 178, 110], supplies={3: 10}, equipment=[], zenie=2000))
