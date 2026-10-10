"""DEV ONLY: run game natives on a state through the game's own bytecode runner.
usage: python3 natives.py IN.ss OUT.ss "[items]"   items: ints are pushes, ('n', id) natives (script_nat_b_lib.asm)"""
import ast
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from common import emu, KEY  # noqa: E402
from script_nat_b_lib import run_code, asm  # noqa: E402

e = emu(os.path.abspath(sys.argv[1]))
e.run(1)
print(run_code(e, asm(ast.literal_eval(sys.argv[3])), max_frames=3000, keys=lambda f: KEY.A if f % 30 < 4 else 0))
e.set_keys(0)
e.run(2)
e.save_file(os.path.abspath(sys.argv[2]))
