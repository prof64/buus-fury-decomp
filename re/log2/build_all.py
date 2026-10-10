"""Regenerate every data/log2/*.json from the ROM, then cross-link moves.json <- enemy attacks.
usage: .venv/bin/python re/log2/build_all.py"""
import subprocess, sys, os, json
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
order = ['extract_formulas.py', 'extract_levels.py', 'extract_rng.py', 'extract_items.py',
         'extract_characters.py', 'extract_moves.py', 'extract_maps.py', 'extract_ai.py', 'extract_enemies.py',
         'extract_bosses.py']
for s in order:
    print('==', s, flush=True)
    subprocess.run([sys.executable, os.path.join(HERE, s)], check=True, cwd=ROOT)

# enemy attacks: full per-attack params (enemies.json enemy_attacks + projectiles) + which classes use them
D = os.path.join(ROOT, 'data', 'log2')
en = json.load(open(os.path.join(D, 'enemies.json')))
mv = json.load(open(os.path.join(D, 'moves.json')))
proj = {p['key']: p for p in en.get('projectiles', [])}
atk_tbl = {a['id']: a for a in en.get('enemy_attack_table', [])}
out = []
for a in en.get('enemy_attacks', []):
    e = dict(a)
    if 'projectile' in a and a['projectile'] in proj:
        e['projectile_params'] = proj[a['projectile']]
    if a.get('attack_id') in atk_tbl:
        e['attack_table_entry'] = atk_tbl[a['attack_id']]
    e['used_by_classes'] = [{'class_id': c['id'], 'names': c.get('names', [])}
                            for c in en['classes'] if a['key'] in c.get('attack_keys', []) and c.get('used_in_game')]
    out.append(e)
mv['enemy_attacks'] = out
mv['enemy_attacks_note'] = en.get('enemy_attack_note', '') + ' ' + en.get('contact_and_melee_rules', '')
json.dump(mv, open(os.path.join(D, 'moves.json'), 'w'), indent=1)
print('moves.json enemy_attacks:', len(mv['enemy_attacks']))
