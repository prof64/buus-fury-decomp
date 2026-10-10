"""list the live actors of a save state: python3 actors.py <state.ss> [program]"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from route import start_emu, program_keys, status  # noqa: E402
from common_field import objs  # noqa: E402

e = start_emu(sys.argv[1])
if len(sys.argv) > 2:
    for k in program_keys(sys.argv[2]):
        e.set_keys(k)
        e.run(1)
print(status(e))
for o in objs(e):
    if o[2] is not None:
        print(o)
