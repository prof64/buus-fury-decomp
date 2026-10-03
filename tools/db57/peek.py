"""peek.py RUNDIR [--keys keys.json] FRAME...  -- replay a run's key log from its start state; at each frame print the
status + enemies (type, pos, hp, AI state) and write RUNDIR/peek_<frame>.png (+ .ss with --save)"""
import json
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dbdriver import enemies, status  # noqa: E402
from ship_driver import ai_state  # noqa: E402
from route import start_emu  # noqa: E402

a = sys.argv[1:]
run = Path(os.path.abspath(a[0]))
kf = a[a.index('--keys') + 1] if '--keys' in a else 'keys.json'
save = '--save' in a
frames = sorted(int(x) for x in a[1:] if x.isdigit())
log = json.loads((run / kf).read_text())
e = start_emu(str(run / ((log['from'] + '.ss') if log.get('from') else 'start.ss')))
f = 0
for t in frames:
    while f < t and f < len(log['keys']):
        e.set_keys(log['keys'][f])
        e.run(1)
        f += 1
    print(f, status(e), 'keys', log['keys'][f - 1] if f else 0, 'timers', e.r32(0x03001068 + 0x4D4), e.r32(0x03001068 + 0x4D8))
    for o in enemies(e):
        print('   enemy type', o[5], 'pos', o[1], o[2], 'f', o[3], 'hp', o[4], 'ai', [hex(x) for x in ai_state(e, o[0])])
    e.screenshot(str(run / f'peek_{t}.png'), scale=2)
    if save:
        e.save_file(str(run / f'peek_{t}.ss'))
