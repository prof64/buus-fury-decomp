import json, sys
p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/data/menu/%s.json' % sys.argv[1]
d = json.load(open(p))
d['meta']['open'][sys.argv[2]] = int(sys.argv[3])
json.dump(d, open(p, 'w'))
print(d['meta'])
