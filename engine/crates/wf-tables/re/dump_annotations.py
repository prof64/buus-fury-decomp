"""Write engine/crates/wf-tables/data/*.json: the parts of the Buu's Fury mechanics data that are NOT in the ROM
(human analysis, documentation strings, emulator measurements). The Rust extractor reads every ROM-derived value
itself and merges these annotations in.

Sources: the Python research modules in re/buu (registries, curated names, doc constants) and, for emulator
measurements, the measured sub-trees of the current data/buu/*.json (they come from work/buu/*.json caches that only
the emulator can produce). Nothing in here is copied from ROM tables: ROM-derived leaves are stripped.

usage: .venv/bin/python engine/crates/wf-tables/re/dump_annotations.py
"""
import copy
import importlib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
OUT = HERE.parent / 'data'
sys.path.insert(0, str(ROOT / 're' / 'buu'))
REF = ROOT / 'data' / 'buu'


def ref(name):
    return json.loads((REF / name).read_text())


def write(name, obj):
    OUT.mkdir(exist_ok=True)
    (OUT / name).write_text(json.dumps(obj, indent=1, ensure_ascii=False) + '\n')
    print('wrote', name)


def pick(d, keys):
    return {k: copy.deepcopy(d[k]) for k in keys if k in d}


# ------------------------------------------------------------------------------------------- simple docs
def levels():
    write('levels.json', dict(meta=ref('levels.json')['meta']))


def rng():
    write('rng.json', ref('rng.json'))


def formulas():
    f = ref('formulas.json')
    f['level_up'].pop('growth_rows')        # ROM: growth table 0x086B72E0
    write('formulas.json', f)


def characters():
    import extract_characters as ec
    c = ref('characters.json')
    out = pick(c, ['meta', 'level_range', 'stat_points', 'level_up', 'movement', 'fusion', 'form_flag_bits',
                   'record_layout'])
    out['char_names'] = ec.CHAR_NAMES
    out['flag_bits'] = {str(k): v for k, v in ec.FLAG_BITS.items()}
    g = next(ch['growth'] for ch in c['characters'] if 'hp_add' in ch['growth'])
    out['growth_doc'] = pick(g, ['hp_add', 'hp_pct', 'ep_add', 'ep_pct'])
    out['growth_unused_note'] = next(ch['growth']['note'] for ch in c['characters'] if 'note' in ch['growth'])
    out['measured_bonus'] = {str(f['index']): f['measured_bonus'] for f in c['forms'] if 'measured_bonus' in f}
    write('characters.json', out)


def moves():
    import extract_moves as em
    m = ref('moves.json')
    out = dict(meta=m['meta'], skill_levels=m['skill_levels'],
               melee=pick(m['melee'], ['input', 'hit', 'rect_format', 'after', 'ep_gain_on_hit', 'crit', 'status']))
    out['melee_keys'] = list(m['melee'].keys())
    rules = {}
    for sid, r in em.COST_RULES.items():
        r = dict(r)
        if 'levels_table' in r:
            r['levels_table'] = list(r['levels_table'])     # [kind, offset into the cost table 0x0871DA98]
        rules[str(sid)] = r
    out['cost_rules'] = rules
    s0 = m['skills'][0]
    out['ep_cost_scaled_by'] = s0['ep']['ep_cost_scaled_by']
    out['measured'] = {str(s['id']): s['measured'] for s in m['skills']}
    write('moves.json', out)


# per-skill EP cost specs of extract_attacks.COST: ('bytes'|'u32', offset into 0x0871DA98) or fixed values
ATTACK_COST_TABLES = {2: ('bytes', 0x00), 6: ('bytes', 0x03), 10: ('bytes', 0x06), 11: ('bytes', 0x09),
                      15: ('bytes', 0x0C), 16: ('bytes', 0x0F), 4: ('u32', 0x18), 8: ('u32', 0x30), 17: ('u32', 0x48)}


def attacks():
    import extract_attacks as ea
    a = ref('attacks_detail.json')
    out = {}
    out['melee'] = {k: v for k, v in a['melee'].items() if k not in ('rect_table', 'rects_rel_player')}
    # form names are ROM strings (form table 0x086B82EC): filled in by the extractor
    out['melee']['forms'] = [{k: v for k, v in f.items() if k != 'name'} for f in out['melee']['forms']]
    out['melee_keys'] = list(a['melee'].keys())
    for k in ('block', 'energy_block', 'dash', 'hit_struct'):
        out[k] = a[k]
    out['top_keys'] = list(a.keys())
    # sub names: ROM skill names where they are one (stored as the skill id), else curated text
    names = {}
    for sub, nm in a['attack_sub_names'].items():
        sid = next((s['id'] for s in a['skills'] if s['name'] == nm), None)
        names[sub] = dict(skill=sid) if sid is not None else dict(text=nm)
    out['attack_sub_names'] = names
    out['ki_blast'] = {k: v for k, v in a['ki_blast'].items() if k not in ('dir_table', 'velocity_px_per_frame')}
    out['ki_blast_keys'] = list(a['ki_blast'].keys())
    out['beams'] = {k: v for k, v in a['beams'].items() if k not in ('level_defs', 'drain_table')}
    out['beams_keys'] = list(a['beams'].keys())
    out['big_bang'] = {k: v for k, v in a['big_bang'].items() if k != 'charge_rate'}
    out['big_bang_keys'] = list(a['big_bang'].keys())
    cost = {}
    for sk, c in ea.COST.items():
        c = dict(c)
        if 'levels' in c:
            kind, off = ATTACK_COST_TABLES[sk]
            c['levels'] = [kind, off]
            c.pop('addr')
        cost[str(sk)] = c
    out['cost'] = cost
    out['kind'] = {str(k): v for k, v in ea.KIND.items()}
    out['state_vt'] = {str(k): hex(v) for k, v in ea.STATE_VT.items()}
    out['skill_measured'] = {str(s['id']): s['measured'] for s in a['skills']}
    out['skill_keys'] = {str(s['id']): list(s.keys()) for s in a['skills']}
    write('attacks.json', out)


def items():
    import extract_items as ei
    i = ref('items.json')
    write('items.json', dict(meta=i['meta'], extra_effects={str(k): v for k, v in ei.EXTRA_EFFECTS.items()}))


def enemies():
    import extract_enemies as ee
    e = ref('enemies.json')
    write('enemies.json', dict(
        meta=e['meta'], stats_note=e['enemies'][0]['stats_note'],
        qualified={str(k): v for k, v in ee.QUALIFIED.items()},
        descriptive={str(k): v for k, v in ee.DESCRIPTIVE.items()},
        rom_text_names={str(k): dict(string_id=v[0], evidence=v[1]) for k, v in ee.ROM_TEXT_NAMES.items()},
        reactions={str(k): v for k, v in ee.REACTIONS.items()},
        loot_samples=20000, loot_seed=1))


def jsonable(v):
    if isinstance(v, dict):
        return {(hex(k) if isinstance(k, int) else str(k)): jsonable(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [jsonable(x) for x in v]
    return v


def ai():
    from ai_decode import REGISTRY, UNIT_DESC
    import extract_ai as ex
    a = ref('enemy_ai.json')
    reg = {}
    for fn, r in REGISTRY.items():
        e = dict(name=r['name'], kind=r.get('kind'), status=r.get('status'), sem=r.get('sem'))
        if 'params' in r:
            e['params'] = [list(p) for p in r['params']]     # [def offset, name, unit]
        if 'children' in r:
            e['children'] = r['children'] if isinstance(r['children'], str) else list(r['children'])
        for k in ('timing', 'projectile', 'on_hit'):
            if k in r:
                e[k] = jsonable(r[k])
        reg[hex(fn)] = e
    write('ai.json', dict(meta=a['meta'], registry=reg, unit_desc=UNIT_DESC,
                          attack_nodes=sorted(ex.ATTACK_NODES), move_nodes=sorted(ex.MOVE_NODES)))


def bosses():
    from ai_decode import BOSS_GROUPS
    b = ref('bosses.json')
    groups = {}
    for g in BOSS_GROUPS:
        m = importlib.import_module(f'boss_facts_{g}')
        groups[g] = dict(
            bosses={hex(int(t)): jsonable(v) for t, v in getattr(m, 'BOSSES', {}).items()},
            projectiles={str(k): jsonable(v) for k, v in getattr(m, 'PROJECTILES', {}).items()},
            specials={str(k): jsonable(v) for k, v in getattr(m, 'SPECIALS', {}).items()},
            projectile_base=jsonable(getattr(m, 'PROJECTILE_BASE', None)))
    write('bosses.json', dict(meta=b['meta'], group_order=list(BOSS_GROUPS), groups=groups,
                              stat_keys=['exp', 'hp', 'ai_state', 'STR', 'POW', 'END', 'resist_melee', 'resist_energy',
                                         'hit_reaction'],
                              stats_units=dict(hp='HP (stored <<10 at obj+0x28C)', exp='XP before multiplier',
                                               resist='128 = x1.0')))


def ai_extra():
    """tools/battle/gen_buu_ai_extra.py registries (it runs main() on import, so parse its source instead)."""
    import ast
    src = (ROOT / 'tools' / 'battle' / 'gen_buu_ai_extra.py').read_text()
    tree = ast.parse(src)
    vals = {}
    for node in tree.body:
        if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name) and \
                node.targets[0].id in ('EXTRA_CREATE', 'AUX_ONLY', 'ONHIT_NAMES'):
            vals[node.targets[0].id] = eval(compile(ast.Expression(node.value), 'gen', 'eval'), {'dict': dict})
    doc = ast.get_docstring(tree, clean=False)

    def conv(d):
        out = {}
        for k, v in d.items():
            v = dict(v)
            if 'params' in v:
                v['params'] = [list(p) for p in v['params']]
            out[hex(k)] = v
        return out
    write('ai_extra.json', dict(
        source='tools/battle/gen_buu_ai_extra.py (ROM decode)',
        onhit_protocol=doc.split('onHit return protocol')[1].split('usage:')[0].strip(),
        onhit_fns={hex(k): v for k, v in vals['ONHIT_NAMES'].items()},
        aux_only=conv(vals['AUX_ONLY']), extra_nodes=conv(vals['EXTRA_CREATE'])))


if __name__ == '__main__':
    only = sys.argv[1:]
    for fn in (levels, rng, formulas, characters, moves, attacks, items, enemies, ai, bosses, ai_extra):
        if not only or fn.__name__ in only:
            fn()
