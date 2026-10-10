"""joinkeys.py OUT.json KEYS1.json KEYS2.json ...: concatenate key logs ({keys: [...]}) into one."""
import json
import sys

keys = []
for p in sys.argv[2:]:
    keys += json.load(open(p))['keys']
json.dump({'from': None, 'keys': keys}, open(sys.argv[1], 'w'))
print(len(keys))
