"""stats_recalc.py: RecalcStats (0x08013C66) called in the emulator on Goku's record -> data/buu/stats_recalc.json
(engine/crates/wf-app/tests/stats_parity.rs).

Cases: every equipment item alone in its slot (as the record is, at base stats 3 - the clamp at 0 -, at 1 HP), the
Saiyan set whole and in parts, stacked effects (the routine's loop does not check the slot), the low-HP item 153 at
its limits, timed boosts (GD+0x4E0..). Inputs: equipment slots (+0x3C..), base stats (+0x28..), HP / max HP, the
boost. Outputs: what the routine wrote (+0x4C.. effective stats and factors, +0x13 flags).
Needs work/states/buu_fight_swforest.ss and the extracted content's equipment list (engine/target*/tmp/*/equipment)."""
import glob
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import *  # noqa: E402,F401,F403
from emutools import call  # noqa: E402

REC = 0x03001560
GD = 0x03001068
SLOTS = ['body', 'hands', 'feet', 'accessory']


def items():
    for d in sorted(glob.glob(str(ROOT / 'engine' / 'target*' / 'tmp' / '*' / 'equipment'))):
        out = [json.load(open(f)) for f in sorted(glob.glob(d + '/*.json'))]
        out = [x for x in out if 'item' in x]
        if out:
            return out
    raise SystemExit('no extracted equipment list')


def main():
    e = emu('buu_fight_swforest.ss')
    st = e.save()
    its = items()
    si = {d['item']: SLOTS.index(d['slot']) for d in its}
    none = [0, 44, 77, 112]

    def run(slots, stats=None, hp=None, max_hp=None, boost=None):
        e.load(st)
        for k in range(4):
            e.w32(REC + 0x3C + 4 * k, slots[k])
        if stats:
            for k, v in enumerate(stats):
                e.w8(REC + 0x28 + k, v)
        if max_hp is not None:
            e.w16(REC + 0x18, max_hp)
        if hp is not None:
            e.w16(REC + 0x16, hp)
        if boost:
            e.w32(GD + 0x4DC, 600)
            for k, v in enumerate(boost):
                e.w32(GD + 0x4E0 + 4 * k, v)
        c = dict(slots=slots, stats=[e.r8(REC + 0x28 + k) for k in range(4)], hp=e.r16(REC + 0x16), max_hp=e.r16(REC + 0x18), boost=boost)
        call(e, 0x08013C66, (REC, 0, 0, 0))
        b = e.read(REC + 0x4C, 0x1C)
        h = lambda o: struct.unpack_from('<H', b, o - 0x4C)[0]  # noqa: E731
        c.update(str=h(0x4C), pow=h(0x4E), end=h(0x50), end2=h(0x52), spd=h(0x54), ep_cost=b[0x56 - 0x4C], ep_block=b[0x57 - 0x4C],
                 xp=h(0x58), thorns=h(0x5C), block=h(0x60), ss=h(0x62), regen=h(0x64), crit=b[0x66 - 0x4C], flags=e.r8(REC + 0x13))
        return c

    def put(*ids):
        s = list(none)
        for i in ids:
            s[si[i]] = i
        return s

    cases = [run(none)]
    for d in its:
        i = d['item']
        cases += [run(put(i)), run(put(i), stats=[3, 3, 3, 3]), run(put(i), hp=1)]
    for s in ([40, 73, 107, 112], [40, 73, 77, 112], [40, 44, 107, 112], [0, 73, 107, 112], [35, 63, 101, 140], [61, 104, 110, 112],
              [21, 60, 77, 112], [16, 16, 16, 16], [34, 34, 77, 112], [20, 20, 42, 42], [153, 153, 77, 112]):
        cases += [run(s), run(s, hp=1)]
    for hp, mx in ((6, 135), (7, 135), (5, 100), (6, 100), (0, 135), (50, 1000), (51, 1000)):
        cases.append(run(put(153), hp=hp, max_hp=mx))
    for b in ([25, 25, 25, 25], [10, 0, 0, 5], [250, 250, 250, 250]):
        cases += [run(none, boost=b), run(put(40, 73, 107), boost=b), run(none, stats=[255, 255, 255, 255], boost=b)]
    about = ("RecalcStats 0x08013C66 called in the emulator on Goku's record (re/buu/stats_recalc.py): the record's inputs "
             "(equipment slots +0x3C.., base stats +0x28.., HP / max HP, the timed boost GD+0x4E0..) and what it wrote "
             "(+0x4C.. effective stats and factors, +0x13 flags)")
    (ROOT / 'data' / 'buu' / 'stats_recalc.json').write_text(json.dumps(dict(about=about, cases=cases), separators=(',', ':')))
    print(len(cases), 'cases')


if __name__ == '__main__':
    main()
