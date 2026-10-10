"""Print markdown tables (enemies, levels) for notes.md from data/log1/*.json."""
import json
from common import DATA

en = json.loads((DATA / 'enemies.json').read_text())['enemies']
print('| id | template name | sprites | HP | Ki | Def | EXP | speed (1/16 px/f) | aggro tiles | move/target/fight | flying | ROM |')
print('|---|---|---|---|---|---|---|---|---|---|---|---|')
for t in en:
    print(f"| {t['id']} | {t['name']} | {', '.join(s['sprite_name'] for s in t['sprites'])} | {t['hp']} | {t['ki']} | "
          f"{t['defense']} | {t['exp_reward']} | {t['speed_raw_sixteenths']} | {t['aggro_tiles']} | "
          f"{t['move_type']}/{t['target_type']}/{t['fight_style']} | {t['flying']} | {t['rom_addr']} |")
print()
lv = json.loads((DATA / 'levels.json').read_text())['levels']
print('| Lv | EXP | max HP | max Ki | flight | Ki regen/16f | ki blast dmg | punch dmg |')
print('|---|---|---|---|---|---|---|---|')
for r in lv:
    print(f"| {r['level']} | {r['exp_required']} | {r['max_hp']} | {r['max_ki']} | {r['max_flight_energy']} | "
          f"{r['ki_regen_per_16_frames']} | {r['ki_blast_damage']} | {r['melee_damage_min']}-{r['melee_damage_max']} |")
