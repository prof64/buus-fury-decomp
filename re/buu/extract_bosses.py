"""Generate data/buu/bosses.json: boss / special-enemy behaviour for a battle simulator.

Sources (all regenerate from the ROM + emulator checks):
  * boss_facts_<group>.py (group in ai_decode.BOSS_GROUPS): hand-decoded per-boss phases, attacks,
    movement, specials, spawn chains and projectile classes, each claim tagged VERIFIED / DISASM / INFERRED.
  * boss_nodes_<group>.py: node registries (merged into the decoder by ai_decode.py).
  * enemy table 0x08711274 (enemies.json) for stats, and the decoded behaviour trees (enemy_ai.json,
    re-run extract_ai.py first so boss nodes are named).

Run order: extract_enemies.py -> extract_ai.py -> extract_bosses.py.
"""
import importlib
import json
from common import *
from ai_decode import BOSS_GROUPS, REGISTRY

OUT = DATA / 'bosses.json'
STAT_KEYS = ('exp', 'hp', 'ai_state', 'STR', 'POW', 'END', 'resist_melee', 'resist_energy', 'hit_reaction')


def jsonable(v):
    if isinstance(v, dict):
        return {(hex(k) if isinstance(k, int) else str(k)): jsonable(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [jsonable(x) for x in v]
    return v


def main():
    enemies = {e['type']: e for e in json.loads((DATA / 'enemies.json').read_text())['enemies']}
    ai = json.loads((DATA / 'enemy_ai.json').read_text())
    ai_en = {e['type']: e for e in ai['enemies']}
    states = dict(ai['states']) if isinstance(ai['states'], list) else ai['states']
    bosses, projectiles, base, groups, specials = {}, {}, {}, {}, {}
    for g in BOSS_GROUPS:
        try:
            m = importlib.import_module(f'boss_facts_{g}')
        except ImportError:
            groups[g] = 'missing'
            continue
        groups[g] = dict(bosses=len(getattr(m, 'BOSSES', {})), projectiles=len(getattr(m, 'PROJECTILES', {})))
        for k, v in getattr(m, 'PROJECTILES', {}).items():
            projectiles[str(k)] = dict(jsonable(v), group=g)
        for k, v in getattr(m, 'SPECIALS', {}).items():
            specials[str(k)] = dict(jsonable(v), group=g) if isinstance(v, dict) else jsonable(v)
        if hasattr(m, 'PROJECTILE_BASE'):
            base = jsonable(m.PROJECTILE_BASE)
        for t, b in getattr(m, 'BOSSES', {}).items():
            t = int(t)
            en = enemies.get(t, {})
            a = ai_en.get(t, {})
            entry = dict(type=t, type_hex=hex(t), group=g, name=b.get('name') or en.get('name'),
                         stats={k: en.get(k) for k in STAT_KEYS},
                         stats_units=dict(hp='HP (stored <<10 at obj+0x28C)', exp='XP before multiplier',
                                          resist='128 = x1.0'),
                         initial_state=a.get('initial_state'),
                         ai_states={s: states.get(s, {}).get('text') for s in a.get('states', [])})
            entry.update(jsonable({k: v for k, v in b.items() if k != 'name'}))
            bosses[hex(t)] = entry
    nodes = {hex(fn): dict(name=r['name'], kind=r.get('kind'), status=r.get('status'), sem=r.get('sem'),
                           params=[dict(offset=hex(o), name=n, unit=u) for o, n, u in r.get('params', [])],
                           **({'timing': jsonable(r['timing'])} if 'timing' in r else {}),
                           **({'projectile': r['projectile']} if 'projectile' in r else {}))
             for fn, r in sorted(REGISTRY.items())}
    out = dict(meta=dict(generator='re/buu/extract_bosses.py', groups=groups,
                         force_spawn='re/buu/forcespawn.py: patch a SW-forest spawn record in ROM with the '
                                     "target's own record, warp in from buu_snakeway_start.ss",
                         units=dict(speed='px per frame unless suffixed _256 (1/256 px)', time='frames (60/s)',
                                    distance='px', chance='percent per frame unless stated'),
                         status_tags='VERIFIED = observed in the emulator; DISASM = read from code; '
                                     'INFERRED = interpretation'),
               projectile_base=base, projectiles=projectiles, regular_enemy_specials=specials, bosses=bosses,
               node_registry=nodes)
    OUT.write_text(json.dumps(out, indent=1))
    print('wrote', OUT, len(bosses), 'bosses', len(projectiles), 'projectiles', len(nodes), 'nodes', groups)


if __name__ == '__main__':
    main()
