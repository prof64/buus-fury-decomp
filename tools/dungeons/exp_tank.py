"""experiment: Trunks vs the 9-5 Juggernaut (from work/db/db1/b2_9-4.ss): mode 'objs' lists the world objects while
it shoots; mode 'rush' = walk up its column, block (R, facing it) while a shell flies, punch upward at point-blank."""
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 're' / 'buu' / 'ship'))
sys.path.insert(0, str(ROOT / 'tools' / 'field'))
from ship_driver import ShipDriver, status, enemies, ai_state, KEY  # noqa: E402
from route import start_emu  # noqa: E402
import common_field as cf  # noqa: E402

mode = sys.argv[1]
e = start_emu(str(ROOT / 'work/db/db1/b2_9-4.ss'))
d = ShipDriver([('nav', 9, 5, False), ('walk', 96, 372), ('walk', 96, 310), ('face', 'U')])
while not d.done():
    e.set_keys(d.keys(e))
    e.run(1)
rec = 0x03001560 + 0x68 * 3
OBJW = cf.OBJW


def world(e):
    out = []
    n = e.r32(OBJW + 4)
    for k in range(min(n, 255)):
        o = e.r32(OBJW + 8 + 4 * k)
        out.append((o, e.r32(o)))
    return out


if mode == 'objs':
    base = Counter(vt for _, vt in world(e))
    for t in range(300):
        e.set_keys(0)
        e.run(1)
        c = Counter(vt for _, vt in world(e))
        new = {('%08X' % k): v for k, v in (c - base).items()}
        if t % 4 == 0:
            en = enemies(e)
            extra = []
            for o, vt in world(e):
                if vt not in base:
                    extra.append(('%08X' % vt, [cf.s32v(e.r32(o + 0x14 + 4 * i)) for i in range(4)]))
            print(t, 'hp', e.r16(rec + 0x16), [hex(x) for x in ai_state(e, en[0][0])], new, extra)

SHELL = 0x0804A368
if mode == 'rush':
    per = int(sys.argv[2]) if len(sys.argv) > 2 else 10
    mash = 0
    for t in range(8000):
        s = status(e)
        en = enemies(e)
        k = 0
        if en and s['p'] is not None:
            x, y = s['p'][0], s['p'][1]
            ex, ey = en[0][1], en[0][2]
            shell = any(vt == SHELL for _, vt in world(e))
            if shell and y - ey > 40:
                k = KEY.R if s['p'][2] == 1 else KEY.UP
            elif y - ey > int(sys.argv[3]) if len(sys.argv) > 3 else 30:
                k = KEY.UP
                mash = 0
            else:
                if s['p'][2] != 1:
                    k = KEY.UP
                else:
                    mash += 1
                    k = KEY.A if mash % per < 2 else 0
        e.set_keys(k)
        e.run(1)
        if t % 50 == 0:
            print(t, status(e)['p'], 'hp', e.r16(rec + 0x16), [(o[1], o[2], o[4] >> 10, [hex(q) for q in ai_state(e, o[0])]) for o in en])
            if not en or e.r16(rec + 0x16) == 0:
                break
    e.screenshot('/tmp/db_tank.png', 1)
