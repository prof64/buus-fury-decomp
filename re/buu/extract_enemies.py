"""Generate data/buu/enemies.json: every enemy/boss type (0x08711274, 150 x 28 bytes) with stats, names,
spawn locations, graphics ids, drop parameters + simulated loot, and AI entry points.

Names: the Capsule Corp database (0x0874D380, 16-byte rows {name, desc, gfx_id, portrait, link}) links an
entry to an enemy type through its last field. Types with no database row get a name via the graphics id
used by their spawn records (spawn gfx id == database gfx id) and are flagged name_source='gfx'.
Loot: enemy drop scripts are all `run_script(497, loot_level, forced_item)`; per-kill expectations are
Monte-Carlo simulated with the Python VM re-implementation (vm.py/loot.py; 20000 kills per level).
AI: initial AI state id (+8) indexes 0x0870F514; decoded trees are in enemy_ai.json (extract_ai.py).
"""
import json
from collections import defaultdict
from common import *
from tables import *
from scriptdis import dis_script
from spawns import scan as scan_spawns
from loot import simulate

REACTIONS = {0x08708920: 'standard (knockback + flinch, behaviour def 0x08708920)', 0x080716F4: 'none (does not flinch)'}


def drop_params(addr):
    if not addr:
        return None
    lines = [s for _, s in dis_script(addr, 40)]
    if lines and lines[0].startswith('pushn [497,'):
        v = [int(x) for x in lines[0][len('pushn ['):-1].split(',')]
        return dict(script=hex(addr), loot_level=v[1], forced_item=v[2] if v[2] >= 0 else None)
    return dict(script=hex(addr), raw=lines)


def main(samples=20000):
    db = db_entries()
    by_link = defaultdict(list)
    by_gfx = defaultdict(list)
    for d in db:
        by_link[d['link']].append(d)
        by_gfx[d['gfx_id']].append(d)
    spawns = scan_spawns()
    sp_by_type = defaultdict(list)
    for s in spawns:
        sp_by_type[s['type']].append(s)
    loot_cache = {}
    enemies = []
    for t in range(N_ENEMY):
        en = enemy_type(t)
        names = by_link.get(t, [])
        gfx_ids = sorted({s['gfx'] for s in sp_by_type.get(t, [])})
        name_source = 'database' if names else None
        if not names:
            for g in gfx_ids:
                if by_gfx.get(g):
                    names = by_gfx[g]
                    name_source = 'gfx'
                    break
        en['name'] = names[0]['name'] if names else None
        en['name_source'] = name_source
        en['database'] = [dict(index=d['index'], name=d['name'], gfx_id=d['gfx_id'], portrait=d['portrait'],
                               description=d['description']) for d in by_link.get(t, [])]
        en['gfx_ids'] = gfx_ids
        en['gfx_defs'] = {g: hex(u32(GFX_TABLE + 4 * g)) for g in gfx_ids if g < 0x200}
        en['hit_reaction'] = REACTIONS.get(int(en['hit_reaction'], 16), en['hit_reaction'])
        dp = drop_params(int(en['drop_script'], 16))
        en['drops'] = dp
        if dp and 'loot_level' in dp:
            key = (dp['loot_level'], dp['forced_item'] if dp['forced_item'] is not None else -1)
            if key not in loot_cache:
                loot_cache[key] = simulate(key[0], key[1], n=samples)
            sim = loot_cache[key]
            dp['simulated_per_kill'] = dict(avg_drops=round(sim['avg_drops'], 3), avg_zenie=round(sim['avg_zenie'], 2),
                                            top=dict(list(sim['per_kill'].items())[:15]))
        en['ai'] = dict(initial_state=en['ai_state'], state_def=hex(u32(AI_STATES + 8 * en['ai_state'])),
                        see='enemy_ai.json')
        locs = defaultdict(int)
        for s in sp_by_type.get(t, []):
            locs[(s['area'], s['world'], s['map'])] += 1
        en['spawns'] = [dict(area=a, world=w, map=m, count=c) for (a, w, m), c in sorted(locs.items())]
        en['spawn_points'] = [dict(world=s['world'], map=s['map'], x=s['x'], y=s['y'], gfx=s['gfx'], condition=s['cond'],
                                   record=s['record']) for s in sp_by_type.get(t, [])]
        en['stats_note'] = 'STR/POW feed the attack factor, END is used for both melee and energy defense (obj+0x1B4 = obj+0x1B6)'
        enemies.append(en)
    # unnamed types that share XP/HP with a neighbouring named type are variants of it (INFERRED)
    for i, en in enumerate(enemies):
        if en['name']:
            continue
        for j in (i + 1, i - 1):
            if 0 <= j < len(enemies) and enemies[j]['name'] and (enemies[j]['exp'], enemies[j]['hp']) == (en['exp'], en['hp']) \
                    and en['hp']:
                en['name'] = enemies[j]['name'] + ' (variant)'
                en['name_source'] = 'inferred: same XP/HP as type %#x' % enemies[j]['type']
                break
        if not en['name'] and en['hp'] == 0:
            en['name_source'] = 'no HP: scripted/non-combat actor'
    out = dict(
        meta=dict(generator='re/buu/extract_enemies.py', table=hex(ENEMY_TYPES),
                  layout={'+0x00': 'u32 XP reward (x player XP multiplier record+0x58/256, rounded up; 0x0800F80C)',
                          '+0x04': 'u32 max HP (stored as HP<<10 at obj+0x28C)',
                          '+0x08': 'u32 initial AI state id (table 0x0870F514)',
                          '+0x0C': 'u8 level-ish byte (not read by spawn/damage/death code; display only?) [INFERRED]',
                          '+0x0D': 'u8 STR (obj+0x1B0)', '+0x0E': 'u8 POW (obj+0x1B2)', '+0x0F': 'u8 END (obj+0x1B4 and +0x1B6)',
                          '+0x10': 'u8 melee resist (obj+0x1B8, 128 = x1.0)', '+0x11': 'u8 energy resist (obj+0x1B9)',
                          '+0x12,+0x13': 'u8 unknown', '+0x14': 'ptr hit-reaction behaviour (0x08708920 standard knockback, 0x080716F4 none)',
                          '+0x18': 'ptr drop script (bytecode) or 0'},
                  enemy_object={'+0x1A0/+0x1A4/+0x1A8': 's32 x/y/z (24.8)', '+0x1B0..+0x1B9': 'combat stats (as player)',
                                '+0x224': 'type entry ptr', '+0x228': 'spawn-record per-spawn ptr', '+0x238': 'AI state stack',
                                '+0x28C': 'HP (22.10 fixed)', 'list': 'IWRAM 0x03001BB0 = count, 0x03001BB4.. = enemy pointers'},
                  status='stats VERIFIED (disasm + emu: HP 500, XP 1225 for type 0x44); names via database link VERIFIED; '
                         'loot SIMULATED from ROM bytecode'),
        enemies=enemies)
    p = DATA / 'enemies.json'
    p.write_text(json.dumps(out, indent=1))
    print('wrote', p, len(enemies), 'types', len(spawns), 'spawn records')


if __name__ == '__main__':
    main()
