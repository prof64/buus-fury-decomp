"""Generate data/buu/levels.json: the XP table (0x086B7084) and per-level next-level requirements."""
import json
from common import *
from tables import XP_TABLE


def main():
    rows = []
    for lv in range(50, 201):
        xp = u32(XP_TABLE + 4 * (lv - 50)) if lv > 50 else 0
        rows.append(dict(level=lv, total_xp=xp))
    for i, r in enumerate(rows):
        r['to_next'] = rows[i + 1]['total_xp'] - r['total_xp'] if i + 1 < len(rows) else None
    out = dict(
        meta=dict(table=hex(XP_TABLE), generator='re/buu/extract_levels.py',
                  note=('u32[151] at 0x086B7084; entry i (0..150) = total XP needed for level 50+i (entry 0 = 0). '

                        'GetLevel 0x080135D2: level = 50 + max{i : XP >= t[i]} '
                        '(binary search over 0..150), capped at 200 (GainExp stops adding levels at 200; XP keeps '
                        'accumulating). XP-to-next 0x0801361C = t[L-49] - XP (0 at level >= 200).'),
                  status='VERIFIED (disasm + emu: XP 2200 -> LVL 60, NXT LVL 675; level-up 60->62 at XP 4099)'),
        min_level=50, max_level=200, levels=rows)
    p = DATA / 'levels.json'
    p.write_text(json.dumps(out, indent=1))
    print('wrote', p)


if __name__ == '__main__':
    main()
