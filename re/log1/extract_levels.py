"""Extract the experience / level-up table -> data/log1/levels.json.

Table 0x080114A8: 16-byte rows {s32 exp_required, s32 max_hp, s32 max_ki, s32 max_flight_energy},
terminated by a row with exp_required == 0. Row 0 has exp -1 (always satisfied).
level_from_exp (0x08005C32): idx = 0; while row[idx].exp <= exp and row[idx].exp != 0: idx++; idx -= 1.
On every kill the player's exp (char+0x84) gains the victim's exp reward and 0x08005BFE applies the row:
  maxKi = row.max_ki; maxHP = row.max_hp; HP += (new maxHP - old maxHP); kiRegen = maxKi >> 6;
  enemy-damage cap (0x03000C1C) = maxHP / 3 + 1. Current ki is NOT refilled.
"""
from romdata import *


def main():
    rows = level_table()
    for i, r in enumerate(rows):
        r['ki_regen_per_16_frames'] = r['max_ki'] >> 6
        r['ki_blast_damage'] = r['max_hp'] // 3
        r['melee_damage_min'] = (r['max_hp'] * 384) >> 9
        r['melee_damage_max'] = (r['max_hp'] * 511) >> 9
        r['max_damage_taken_per_hit'] = r['max_hp'] // 3 + 1
        r['exp_to_next'] = rows[i + 1]['exp_required'] - r['exp_required'] if i + 1 < len(rows) else None
    dump('levels.json', dict(
        source='LoG1 (USA) level table 0x080114A8 (re/log1/extract_levels.py)',
        max_level=len(rows),
        units=dict(exp_required='total experience needed to be this level', max_hp='hit points',
                   max_ki='ki points', max_flight_energy='flight energy units; 1 unit per 8 frames of flight',
                   ki_regen_per_16_frames='ki restored every 16th frame (frame_counter & 15 == 0)'),
        notes=['Defense of the player is a constant 250 at every level (set by init_char 0x08006E62).',
               'Level 1 row exp field is -1 (0xFFFFFFFF) so level_from_exp always returns >= 0.',
               '"To Next Level" in the pause menu = row[idx+1].exp - exp (0x08005C62).'],
        levels=rows))


if __name__ == '__main__':
    main()
