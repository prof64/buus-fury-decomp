"""Generate data/buu/moves.json: player melee, energy skills (0x0871DAF8) with EP costs decoded from the
canUse()/activate() functions + cost table 0x0871DA98, emulator-measured damage multipliers / timings
(work/buu/skills_measured*.json from measure_skills.py), and the attack-type table 0x086B8088.

Frame-accurate internals (projectile speeds, charge thresholds, etc.) are in attacks_detail.json
(extract_attacks.py)."""
import json
from collections import defaultdict
from common import *
from tables import *

CT = SKILL_COSTS

# EP rules decoded from each skill's canUse (C) / activate (A) function (see notes.md 'Energy skills').
# 'bytes@off' = u8 cost table at 0x0871DA98+off indexed by skill level, 'words@off' = u32 table.
COST_RULES = {
    1: dict(canuse_min_ep=200, rule='needs EP >= 200 to start; drains while the beam is held'),
    2: dict(levels_table=('bytes', 0x00), rule='needs EP >= cost[level]; charge-and-release'),
    3: dict(canuse_min_ep=12, spend_per_tick=2, rule='needs EP >= 12; activate spends 2; drains while held'),
    4: dict(levels_table=('words', 0x18), rule='needs EP >= cost[level]'),
    5: dict(canuse_min_ep=300, spend=300, rule='spends 300 on activation, then drains while steering the dragon'),
    6: dict(levels_table=('bytes', 0x03), rule='needs EP >= cost[level]; drains while held'),
    7: dict(percent_of_max_ep=5, rule='cost = ceil(maxEP*0xCCD/65536) (5%) per shot; activation + each refire'),
    8: dict(levels_table=('words', 0x30), rule='needs/spends cost[level]'),
    9: dict(canuse_min_ep=350, rule='needs EP >= 350 (0x15E); drains 4 EP every other frame while charging'),
    10: dict(levels_table=('bytes', 0x06), rule='needs EP >= cost[level]'),
    11: dict(levels_table=('bytes', 0x09), rule='needs EP >= cost[level]; drains while held'),
    12: dict(rule='Fusion Dance: requires a fusion partner/conditions (0x03002C04 == 0, form flags); no EP'),
    13: dict(canuse_min_ep=160, spend=160, rule='spends 160'),
    14: dict(canuse_min_ep=40, rule='needs EP >= 40 (Instant Transmission); 0x03002C04 must be 0'),
    15: dict(levels_table=('bytes', 0x0C), rule='needs EP >= cost[level]; drains while held'),
    16: dict(levels_table=('bytes', 0x0F), rule='needs EP >= cost[level]; drains while held'),
    17: dict(levels_table=('words', 0x48), rule='needs/spends cost[level] (Masenko)'),
    18: dict(rule='Saiyaman: toggles Great Saiyaman form (always usable)'),
    19: dict(rule='unused slot (duplicate Saiyaman entry, canUse returns 0)'),
    20: dict(canuse_min_ep=10, rule='Super Saiyan: needs EP >= 10 and record+4 (SS cooldown) == 0; second press reverts'),
    21: dict(rule='unused slot (duplicate SS entry)'),
    22: dict(rule='Super Saiyan 3: must already be SS (form flag 0x01) - see canUse 0x0801A55F'),
    23: dict(canuse_min_ep=350, spend=350, rule='Ultra Volleyball: spends 350 (0x15E)'),
}


def table_vals(kind, off):
    if kind == 'bytes':
        return [u8(CT + off + i) for i in range(3)]
    return [u32(CT + off + 4 * i) for i in range(3)]


def measured():
    out = defaultdict(list)
    for fn in ('skills_measured.json', 'skills_measured2.json'):
        p = WORK / fn
        if p.exists():
            for r in json.loads(p.read_text()):
                out[r['skill']].append(r)
    return out


def main():
    meas = measured()
    skills = []
    for sid in range(1, 0x18):
        s = skill(sid)
        rule = dict(COST_RULES.get(sid, {}))
        if 'levels_table' in rule:
            k, off = rule.pop('levels_table')
            rule['ep_cost_by_level'] = table_vals(k, off)
            rule['cost_table_address'] = hex(CT + off)
        rule['ep_cost_scaled_by'] = 'record+0x56/16 (Dragon Armor halves); while SS/SS3 the cost is taken from the SS/SS3 gauge'
        s['ep'] = rule
        runs = meas.get(sid, [])
        by_level = defaultdict(list)
        for r in runs:
            by_level[r['level']].append(r)
        ms = {}
        for lv, rs in sorted(by_level.items()):
            hits = [r for r in rs if r['hits']]
            ms[lv] = dict(
                damage_multipliers=sorted({m for r in hits for m in r['mults']}),
                attack_type_subs=sorted({tuple(x) for r in hits for x in r['subs']}),
                first_hit_frame=min((r['first_hit'] for r in hits), default=None),
                runs=[dict(pattern=r['pattern'], dist=r.get('dist', 60), hits=r['hits'], first_hit=r['first_hit'],
                           last_hit=r['last_hit'], total_damage=r['total_dmg'], ep_spent=r['ep_total'],
                           ep_events=r['ep_events'][:12]) for r in rs])
        s['measured'] = dict(
            setup='Goku (STR/POW 27) vs Majin Fighter (END 31) 60 px below (dist field), maxEP 999; '
                  'patterns: tap = B for 3 frames, holdN = B held N frames, mash = B 3/12 frames; '
                  'frames counted from the first frame of input; VERIFIED (emu)',
            by_level={str(k): v for k, v in ms.items()})
        skills.append(s)
    melee = dict(
        input='A (pressed edge) from idle/walk state (0x08013022 / 0x080131BA) -> 0x08012A24',
        hit='instant hit test on press: hit struct {dir, mult=0x400, attack type 0x086B8098 (melee sub 2), attacker} vs '
            'enemies overlapping the facing rect',
        rects_relative_to_player_pos={['down', 'up', 'left', 'right'][d]: rect(MELEE_RECT, d) for d in range(4)},
        rect_format='x0, y0, x1, y1 (player pos x = left edge of the 16-px body, y = feet)',
        after='player state 4/5/6 (random 4+rand(3)) = one of three punch/kick animations; Goku: 17 frames each before idle '
              '(VERIFIED emu, t_punchtime.py); next A only accepted in idle/walk',
        damage_type=attack_type(2),
        ep_gain_on_hit='+1 EP per melee hit that deals damage (script #2; Armor of Light +2, Talisman of Light +1 more, '
                       'Armor of Darkness / Vampire Cape: +1 HP instead)',
        crit='chance (82*STR + crit_bonus)/65536 per melee hit unless attack sub == 3; crit = power*1.5',
        status='VERIFIED (disasm + emu)')
    out = dict(
        meta=dict(generator='re/buu/extract_moves.py', skill_table=hex(SKILLS), cost_table=hex(CT),
                  attack_type_table=hex(ATTACK_TYPES),
                  skill_entry_layout={'+0x00': 'u32 max upgrade level (2) or 0xFF (not upgradable)',
                                      '+0x04/+0x08': 'ptr icons', '+0x0C': 'u16 name string', '+0x0E': 'u16 description string',
                                      '+0x10': 'ptr canUse(record) -> bool', '+0x14': 'ptr activate(record, player)'},
                  damage='see formulas.json damage; mult is per hit (1024 = x1.0), energy moves use POW and the target END2'),
        melee=melee,
        skills=skills,
        attack_types=[attack_type(i) for i in range(18)],
        skill_levels=dict(storage='record+0x2E+2*slot (and +0x2F)', values='0..2 shown as Level1..Level3',
                          set_by='script native (0x08003936 -> 0x08013B54) i.e. story/trainer scripts; not by use (no '
                                 'writes observed while using skills)', selected='record+0x15 (cycled by 0x08013B34)'))
    p = DATA / 'moves.json'
    p.write_text(json.dumps(out, indent=1))
    print('wrote', p)


if __name__ == '__main__':
    main()
