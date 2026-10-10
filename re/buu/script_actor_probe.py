"""script_actor_probe.py state id [id...] -- resolve script actor ids with the game's own lookup 0x080001C6
(injected call) and print the object, its vtable and sprite def (+0x58)."""
import sys
from pathlib import Path
from common import *
from emutools import call

st = sys.argv[1]
e = emu(st if (STATES / st).exists() or Path(st).is_absolute() else WORK / st)
e.run(2)
for a in sys.argv[2:]:
    i = int(a, 0)
    o = call(e, 0x080001C6, (i & 0xFFFFFFFF, 0, 0, 0))
    d = call(e, 0x08000158, (i & 0xFFFFFFFF, 0, 0, 0)) if i >= 0 else 0
    print(f'actor {i}: obj {o:#x}', f'vt {e.r32(o):#x} def {e.r32(o + 0x58):#x}' if o else '', f'def_for_id {d:#x}',
          'player', hex(e.r32(0x03001c34)), 'player def', hex(e.r32(e.r32(0x03001c34) + 0x58)))
