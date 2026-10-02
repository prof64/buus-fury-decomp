"""Regenerate every data/log2/*.json from the ROM, then cross-link moves.json <- enemy attacks.
usage: .venv/bin/python re/log2/build_all.py"""
import subprocess, sys, os, json
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
order = ['extract_formulas.py', 'extract_levels.py', 'extract_rng.py', 'extract_items.py',
         'extract_characters.py', 'extract_moves.py', 'extract_maps.py', 'extract_ai.py', 'extract_enemies.py']
for s in order:
    print('==', s, flush=True)
    subprocess.run([sys.executable, os.path.join(HERE, s)], check=True, cwd=ROOT)

# enemy attacks: which classes use which attack-table ids (from enemies.json)
D = os.path.join(ROOT, 'data', 'log2')
en = json.load(open(os.path.join(D, 'enemies.json')))
mv = json.load(open(os.path.join(D, 'moves.json')))
users = {}
for c in en['classes']:
    for a in c.get('attacks', []):
        users.setdefault(a, []).append({'class_id': c['id'], 'names': c.get('names', [])})
mv['enemy_attacks'] = [{'description': k, 'used_by_classes': v} for k, v in sorted(users.items())]
mv['enemy_attacks_note'] = en.get('enemy_attack_note', '')
json.dump(mv, open(os.path.join(D, 'moves.json'), 'w'), indent=1)
print('moves.json enemy_attacks:', len(mv['enemy_attacks']))
