"""script_trace_item.py [item] [shots] -- item-use trace (natural, through the menu): Snakeway start, give 3 of a
supply (GameData+2+id), lower Goku's HP, open START > Inventory (R), select it with A and confirm.
The supply's use script (data/buu/items.json use_script, an inline script) runs from the menu."""
import sys
from common import *
from script_trace import Tracer

item = int(sys.argv[1]) if len(sys.argv) > 1 else 0
shots = len(sys.argv) > 2
GD = 0x03001068
REC = 0x03001560
e = emu('buu_snakeway_start.ss')
e.run(2)
e.w8(GD + 2 + item, 3)   # supply counts: GameData+2+id (native 0x82 decrements it)
e.w16(REC + 0x16, 40)   # current HP
S = Shots(e, f'trace_item_{item}', cols=6)
tr = Tracer(e)
seq = [(KEY.START, 40), (KEY.R, 40), (KEY.A, 40), (KEY.A, 40), (KEY.A, 40), (KEY.A, 60), (KEY.B, 30), (KEY.B, 30),
       (KEY.B, 30), (KEY.START, 30)]
for k, after in seq:
    tr.run(4, lambda f: k)
    tr.run(after)
    if shots:
        S.snap()
if shots:
    S.write()
print('HP now', e.r16(REC + 0x16), 'count', e.r8(GD + 2 + item))
print(tr.save(f'item_use_{item}', {'state': 'buu_snakeway_start.ss',
                                   'how': f'natural: menu START>R>A>A.. using supply {item} with HP set to 40'}),
      tr.summary())
