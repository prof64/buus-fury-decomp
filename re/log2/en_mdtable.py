import json
d=json.load(open('/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/data/log2/enemies.json'))
print('| id | name(s) | HP | exp | STR | POW | END | melee/energy taken | ai | aggro px (facing?) | chase px/f | attacks | spawns | boss/scripted |')
print('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|')
for c in d['classes']:
    s=c['ai_summary']
    nm=', '.join(c['names'] or ['(unnamed)'])
    if nm.startswith('Gohan'): nm='(player placeholder row)'
    if len(nm)>40: nm='Human NPCs etc.'
    att='; '.join(a.split(' (')[0] for a in c['attacks']) or '-'
    ch=','.join(str(x) for x in s['speeds'].get('ChasePlayer',[])) or '-'
    ag=f"{s['aggro_radius_px']}{' (seen)' if s['aggro_requires_facing'] else ''}" if s['aggro_radius_px'] else '-'
    print(f"| {c['id']} | {nm} | {c['hp']} | {c['exp']} | {c['STR']} | {c['POW']} | {c['END']} | {c['melee_taken_mult']}/{c['energy_taken_mult']} | {c['ai_state']} | {ag} | {ch} | {att} | {c['spawn_count']} | {'yes' if c['is_boss_or_scripted'] else ''} |")
