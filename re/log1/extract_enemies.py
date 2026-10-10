"""Extract every enemy/boss template -> data/log1/enemies.json.

Enemy template table 0x086B6094, 0x40 bytes per record, ids 0..0x21, copied into the 0x278-byte
character struct by init_char (0x08006D90, team 2 branch). Field semantics: see notes.md
"Enemy template format". Sprite -> template map: u8[109] at 0x0815A69C (used by 0x0800573A
and by the script command at 0x0800DB00 that turns an NPC into an enemy / changes its form).
"""
import json
import math
from romdata import *

MOVE_TYPES = {0: 'always-active, stands still (not added to the dormant list)',
              1: 'guard: return to guard tile (template +0x24/+0x26) when idle, else stand',
              2: 'guard (same jump-table entry as 1, roams)',
              3: 'patrol up to 5 waypoints (char+0x240), else random wander',
              4: 'random wander within aggro_tiles (max 14) of current tile',
              5: 'leashed wander: return home (leash_home) when Manhattan distance >= leash_radius'}
TARGET_TYPES = {0: 'never acquires the player by itself (only retaliates)',
                1: 'aggressive: targets player when Manhattan tile distance <= aggro_tiles',
                2: 'assist: targets player only if the player is in its 5-slot hate list (char+0x260)',
                3: 'opportunist: attacks only if the player is already engaged (player char+0x25C != 0), else flees',
                4: 'always flees from its target (0x0800F9AC)'}
FIGHT_STYLES = {0: 'always attack', 1: 'flee while target HP >= ~8/9 of max (tHP + tHP/8 >= tMaxHP), else attack',
                2: 'attack only if own HP > target HP, else flee',
                3: 'attack; when own HP < maxHP/2, 6/101 chance per decision to flee'}


def main():
    names = sprite_names()
    maps = json.loads((DATA / 'maps.json').read_text())['maps'] if (DATA / 'maps.json').exists() else []
    spr2t = [u8(SPRITE_TO_TEMPLATE + i) for i in range(109)]
    out = []
    for i in range(N_TEMPLATES):
        t = template(i)
        sprites = [s for s in range(109) if spr2t[s] == i and not (i == 0 and names[s] != 'squirrel')]
        if i == 0:
            sprites = [names.index('squirrel')]
        t['sprites'] = [dict(sprite_id=s, sprite_name=names[s], sprite_def=sprite_def(u8(SPRITE_TO_DEF + s)))
                        for s in sprites]
        t['speed_px_per_frame'] = speed_class(t['speed_class']) / 16
        t['speed_raw_sixteenths'] = speed_class(t['speed_class'])
        v = t['aggro_tiles']
        r2 = (64 * v * v + (64 * v * v >> 1)) & 0xFFFF
        t['wake_radius_sq_px'] = r2
        t['wake_radius_px'] = round(math.sqrt(r2), 1)
        t['ki_regen_per_16_frames'] = t['ki'] >> 6
        t['melee_damage'] = dict(formula='cur_hp//4 + rand(cur_hp//2)', min_at_full_hp=t['hp'] // 4,
                                 max_at_full_hp=t['hp'] // 4 + t['hp'] // 2)
        t['ki_blast'] = (dict(usable=True, damage='max_hp//3', damage_at_full=t['hp'] // 3,
                              requires='ki > max_ki//16 and target in line',
                              power_spent='rand(ki//2) + ki//4') if t['ki'] > 0 else dict(usable=False))
        t['move_type_desc'] = MOVE_TYPES.get(t['move_type'], '?')
        t['target_type_desc'] = TARGET_TYPES.get(t['target_type'], '?')
        t['fight_style_desc'] = FIGHT_STYLES.get(t['fight_style'], '?')
        t['roams'] = t['move_type'] in (2, 3, 4, 5)
        app = []
        for m in maps:
            n_enemy = sum(1 for a in m['actors'] if a['kind'] == 2 and a['sprite_id'] in sprites)
            n_npc = sum(1 for a in m['actors'] if a['kind'] != 2 and a['sprite_id'] in sprites)
            if n_enemy or n_npc:
                app.append(dict(map=m['index'], placed_enemies=n_enemy, scripted_npc_actors=n_npc))
        t['appears'] = app
        t['drops'] = 'none (no drop code in the death path 0x08007F4C/0x08005E12; death only notifies scripts via 0x08005478)'
        out.append(t)
    dump('enemies.json', dict(
        source='LoG1 (USA) enemy templates 0x086B6094 (0x40 bytes each), decoded by re/log1/extract_enemies.py',
        units=dict(hp='hit points (char+0x4C current / +0x50 max)', ki='ki points (char+0x54/+0x58)',
                   defense='flat value used by apply_damage 0x08007352', exp_reward='added to player exp on kill',
                   speed='SPEED_TABLE 0x080116A4 value in 1/16 px per frame (60 fps)',
                   aggro_tiles='8-px tiles, Manhattan distance for target acquisition; also wander radius',
                   hurtbox='pixels relative to entity position (top-left of the 16x8 foot box), per facing'),
        notes=['Names come from the ROM template table (developer labels); sprite names from 0x080109F0. '
               'Labels and sprites disagree in places (sprite "trex" uses template "strong trex", sprite '
               '"burter" uses template "jeice", sprite "ginyu" uses template "burter", oozaru1/2 use template 6).',
               'Boss actors are placed as NPCs and converted to enemies by scripts (0x0800573A / 0x0800DB00).',
               'devil_2 has HP 0x7FFFFFFF (effectively unkillable).'],
        enemies=out))


if __name__ == '__main__':
    main()
