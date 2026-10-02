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

# Display names. `name` stays the game's own text (Capsule Corp database row, or the ROM string named in
# name_source); several types share one database name, so `display_name` adds the qualifier that tells them apart
# (location / phase / behaviour, from bosses.json and the AI trees). Types the game never names get a descriptive
# display name and display_name_source = 'descriptive'.
QUALIFIED = {
    0x04: 'Babidi (Giant Crater, script-placed)', 0x19: 'Babidi (West City)', 0x1d: 'Babidi (Giant Crater, with Majin Buu)',
    0x0b: 'Broly (phase 2)',
    0x1a: 'Majin Buu (Giant Crater, with Babidi)', 0x1b: 'Majin Buu (Giant Crater, 2nd encounter)',
    0x1c: 'Majin Buu (West City)',
    0x26: 'Goten (tag game)', 0x27: 'Goten (Time Chamber sparring)',
    0x36: 'Janemba Hand (summoned by Janemba)',
    0x3b: 'Human (World Tournament fighter)',
    0x3d: 'Kid Buu (split copy)',
    0x4f: 'Ninja (disguised)', 0x66: 'Shadow Warrior (disguised)', 0x69: 'Shinobi (disguised)',
    0x6d: 'Spopovich (225 HP)', 0x6e: 'Spopovich (225 HP, 2)', 0x6f: 'Spopovich (bear hug, with Yamu)',
    0x70: 'Spopovich (1855 HP, no bear hug)',
    0x93: 'Yamu (EP drain, with Spopovich)', 0x94: 'Yamu (no EP drain)',
    0x73: 'Super Buu (Time Chamber)', 0x74: 'Super Buu (two-phase)', 0x75: 'Super Buu (script-created)',
    0x76: 'Super Buu (Gotenks absorbed)', 0x77: 'Super Buu (Gohan absorbed)', 0x78: 'Super Buu (Gohan absorbed, 0 XP)',
    0x79: 'Super Buu (Brain)',
    0x88: 'Vegeta (Gravity Chamber)', 0x89: 'Vegeta (Barren Wasteland)',
}
# types with no database row whose name is still in the ROM text: (string id, evidence)
ROM_TEXT_NAMES = {0x84: (1140, 'string 1140 "Uub" (database text); the spawn gfx 273 is the Uub sprite')}
# types the game never names (no database row, no text)
DESCRIPTIVE = {
    0x0d: 'Kamikaze ghost (Super Buu minion)', 0x0e: 'Buu fragment (freezes the player)', 0x1e: 'Unnamed fighter 0x1E (unused)',
    0x22: 'Inert helper 0x22', 0x72: 'Inert helper 0x72', 0x7e: 'Inert helper 0x7E',
    0x34: 'Janemba part (arm A)', 0x35: 'Janemba part (arm B)', 0x37: 'Janemba part (upper body)',
    0x54: 'Dummy type 0x54 (unused)', 0x87: 'Unnamed fighter 0x87 (unused)', 0x8d: 'Floor trigger (not a fighter)',
    0x90: 'Worm part 0x90 (unused)',
}

REACTIONS = {0x08708920:'standard (knockback + flinch, behaviour def 0x08708920)', 0x080716F4: 'none (does not flinch)'}


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
    from strtab import S
    for en in enemies:
        t = en['type']
        if not en['name'] and t in ROM_TEXT_NAMES:
            sid, why = ROM_TEXT_NAMES[t]
            en['name'] = S(sid)
            en['name_source'] = why
        if t in QUALIFIED:
            en['display_name'], en['display_name_source'] = QUALIFIED[t], 'game name + qualifier'
        elif t in DESCRIPTIVE:
            en['display_name'], en['display_name_source'] = DESCRIPTIVE[t], 'descriptive'
        elif en['name'] and en['name_source'] and en['name_source'].startswith('inferred'):
            en['display_name'], en['display_name_source'] = en['name'], 'descriptive'
        else:
            en['display_name'], en['display_name_source'] = en['name'], 'game'
    dup = defaultdict(list)
    for en in enemies:
        dup[en['display_name']].append(hex(en['type']))
    clash = {k: v for k, v in dup.items() if len(v) > 1}
    assert not clash, f'duplicate display names: {clash}'
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
