"""The REAL post-game state: replay the real route's last leg (work/ship/sb/m_kid_buu_punched.ss + sbk_kid_buu.json,
segments.md §8) and save the first frame after the ending's flag 149 where the player stands under control on the
field (s463's node 71 leaves Goku at Capsule Corp 4-21 before the "final sequence" trigger s190): no RAM writes.

    .venv/bin/python tools/side/postgame_state.py [OUT.ss]     (default work/side/pg_start.ss)

Also writes OUT's key prefix (work/side/pg_start.keys.json: {'from', 'keys'}) for recording the stretch."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path[:0] = [str(ROOT / 'tools' / 'story_playthrough'), str(ROOT / 'tools' / 'field')]
from route import start_emu, status  # noqa: E402

out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / 'work' / 'side' / 'pg_start.ss'
st = ROOT / 'work' / 'ship' / 'sb' / 'm_kid_buu_punched.ss'
keys = json.loads((ROOT / 'work' / 'ship' / 'sb' / 'sbk_kid_buu.json').read_text())
keys = keys['keys'] if isinstance(keys, dict) else keys
e = start_emu(str(st))
G = 0x03001068


def flag(f):
    return (e.r8(G + 0x55 + (f >> 3)) >> (f & 7)) & 1


calm = 0
for k, key in enumerate(keys):
    e.set_keys(key)
    e.run(1)
    if not flag(149):
        continue
    s = status(e)
    # 30 quiet frames with the player shown, no scene, and no keys pressed by the route: under control
    calm = calm + 1 if (s['p'] is not None and s['scene'] == 0) else 0
    if calm >= 30:
        e.set_keys(0)
        e.save_file(str(out))
        Path(str(out).replace('.ss', '.keys.json')).write_text(json.dumps({'from': st.name, 'keys': keys[:k + 1]}))
        print('frame', k + 1, s)
        break
else:
    print('not found')
