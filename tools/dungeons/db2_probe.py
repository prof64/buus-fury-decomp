"""db2_probe.py STATE PROGRAM [--shot name]: poke.py + the active character's HP / EP / skill slot, the world's
object vtables and the enemies after each step (debugging the dungeon 2 route)."""
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
sys.path.insert(0, str(ROOT / 'tools' / 'dungeons'))
from ship_driver import status, enemies, ai_state  # noqa: E402
from db_driver import world_vts  # noqa: E402
from route import start_emu, keys_of  # noqa: E402

e = start_emu(os.path.abspath(sys.argv[1]))


def show(tag):
    c = e.r8(0x03001068)
    r = 0x03001560 + 0x68 * c
    print(tag, status(e), 'char', c, 'hp', e.r16(r + 0x16), e.r16(r + 0x18), 'ep', e.r16(r + 0x1A), e.r16(r + 0x1C),
          'slot', e.r8(r + 0x15))
    for o in enemies(e):
        print('   enemy', o[1:], [hex(x) for x in ai_state(e, o[0])])
    if '--vts' in sys.argv:
        print('   vts', [hex(v) for v in world_vts(e)])


show('start')
for seg in sys.argv[2].split(','):
    k, n = seg.split(':')
    e.set_keys(keys_of(k))
    e.run(int(n))
    show(seg)
e.set_keys(0)
if '--shot' in sys.argv:
    e.screenshot('/tmp/poke_%s.png' % sys.argv[sys.argv.index('--shot') + 1], 2)
if '--save' in sys.argv:
    e.save_file(sys.argv[sys.argv.index('--save') + 1])
