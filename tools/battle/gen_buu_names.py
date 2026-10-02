"""The game's own names for the playable forms -> data/battle/buu_form_names.json.

The ROM names a form only by its character (form table 0x086B82EC +2: string 'Goku', 'Gohan', ...). The Dojo (versus)
mode has the game's names per look: its roster 0x0872BFCC (32 entries of 28 bytes, u16 gfx id at +0) lists the
characters in the order of the strings 116..147 ('Goku', 'SS Goku', 'SS3 Goku', 'Teen Gohan', 'SS Teen Gohan',
'Mystic Gohan', 'Great Saiyaman', 'Gohan', 'SS Gohan', ...). A form whose gfx id (form +0xE) is in the roster gets
that name; the halo / Other World variants (gfx + 1 of a rostered look) get the same name; the other forms keep their
character name (form +2). The upper-case HUD name (form +4) is kept as `hud`.

usage: .venv/bin/python tools/battle/gen_buu_names.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, 're', 'buu'))
from common import u8, u16  # noqa: E402
from strtab import S  # noqa: E402

FORMS = 0x086B82EC
ROSTER = 0x0872BFCC
ROSTER_N = 32
ROSTER_NAME0 = 116
# halo sprite defs (gfx = def + 9) -> the look they add the halo to: Goku 108 -> 106, SS Goku 110 -> 109, SS3 Goku
# 112 -> 111, Vegeta 271 -> 270, SS Vegeta 274 -> 273
HALO = {117: 115, 119: 118, 121: 120, 280: 279, 283: 282}


def main():
    roster = {}
    for i in range(ROSTER_N):
        g = u16(ROSTER + 28 * i)
        roster.setdefault(g, (S(ROSTER_NAME0 + i), i))
    out = {}
    for f in range(54):
        e = FORMS + 24 * f
        char = S(u16(e + 2))
        hud = S(u16(e + 4))
        g = u16(e + 0xE)
        if g in roster:
            name, i = roster[g]
            src = f'Dojo roster entry {i} (gfx {g}) -> string {ROSTER_NAME0 + i}'
        elif HALO.get(g) in roster:
            # the halo variant of a rostered look (sprite defs with the halo table, GRAPHICS.md 3.2): same name
            name, i = roster[HALO[g]]
            src = f'gfx {g} = the halo variant of Dojo entry {i} (gfx {HALO[g]})'
        else:
            name = char
            src = f'form +2 string {u16(e + 2)} (no Dojo entry for gfx {g})'
        out[str(f)] = {'name': name, 'char': char, 'hud': hud, 'gfx': g, 'source': src}
        print(f, repr(name), '|', src)
    path = os.path.join(ROOT, 'data', 'battle', 'buu_form_names.json')
    with open(path, 'w') as fp:
        json.dump({'source': 'tools/battle/gen_buu_names.py (ROM: form table 0x086B82EC, Dojo roster 0x0872BFCC, strings 116..147)',
                   'forms': out}, fp, indent=1)
    print('wrote', path)


if __name__ == '__main__':
    main()
