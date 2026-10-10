"""tilepeek.py SEG.json: a cut's start - the main-loop mode object (0x03000F90), scene depth, the world objects' vtables,
the first frames' keys and fade probe (where a 'no leader' cut stands)"""
import json
import struct
import sys

p = sys.argv[1]
L = open(p).read().split('\n')
head = json.loads(L[0].rstrip(',') + ']}') if L[0].endswith('[') else None
doc = json.loads(open(p).read())
init = doc['init']
ram = init['ram']
print('keys of init', list(init.keys()))
print('ram keys', list(ram.keys()) if isinstance(ram, dict) else type(ram))
fr = doc['frames']
print('keys', [f['keys'] for f in fr[:40]])
print('io5', [f.get('io', [None] * 6)[5] for f in fr[:40]])
print('objs', [(o['vt'], o['sprite']) for o in init.get('objs', [])])
