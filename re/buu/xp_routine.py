"""xp_routine.py: the game's XP routines called in the emulator -> data/buu/xp_routine.json
(engine/crates/wf-app/tests/stats_parity.rs `xp_routine_matches_the_game`).

* the script native give_xp's wrapper 0x080043D8 (GameData, xp): the current character's record gets the amount as
  it is - whatever XP multiplier (+0x58, weighted clothing) the record has;
* the routine 0x08013914 (record, xp) on a record: at 0 HP nothing, a form without the XP flag nothing, at level 200
  the XP still counts (no level check), below a level's threshold the XP only.
(Amounts that reach a level are not called here: the routine then runs the level-up hook as a nested scene.)
Needs work/states/buu_fight_swforest.ss."""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import *  # noqa: E402,F401,F403
from emutools import call  # noqa: E402

REC = 0x03001560
GD = 0x03001068


def main():
    e = emu('buu_fight_swforest.ss')
    st = e.save()
    out = []

    def case(name, fn, xp, setup=None):
        e.load(st)
        if setup:
            setup()
        call(e, 0x08013C66, (REC, 0, 0, 0))
        before = dict(level=e.r8(REC + 0x15) if False else None)
        x0, hp, form, mult = e.r32(REC + 0x24), e.r16(REC + 0x16), e.r8(REC + 0x14), e.r16(REC + 0x58)
        lv0 = call(e, 0x080135D2, (REC, 0, 0, 0)) & 0xFF
        e.load(st)
        if setup:
            setup()
        call(e, 0x08013C66, (REC, 0, 0, 0))
        if fn == 'native':
            call(e, 0x080043D8, (GD, xp, 0, 0))
        else:
            call(e, 0x08013914, (REC, xp, 0, 0))
        out.append(dict(name=name, fn=fn, xp=xp, hp=hp, form=form, mult=mult, level=lv0, xp_before=x0, xp_after=e.r32(REC + 0x24)))

    assert e.r8(GD) * 0x68 + GD + 0x4F8 == REC, 'the current character is the record'
    for item, slot in ((0, None), (106, 2), (72, 1), (52, 1)):
        def eq(item=item, slot=slot):
            if slot is not None:
                e.w32(REC + 0x3C + 4 * slot, item)
        case(f'native item {item}', 'native', 7, eq)
        case(f'routine item {item}', 'routine', 7, eq)
    case('0 hp', 'routine', 7, lambda: e.w16(REC + 0x16, 0))
    case('0 hp native', 'native', 7, lambda: e.w16(REC + 0x16, 0))
    for f in (45, 47, 53):
        case(f'form {f}', 'routine', 7, lambda f=f: e.w8(REC + 0x14, f))
    # LevelUp 0x08013678 (record, old level, new level): the growth of each main character's form, one and several
    # levels at once, small and large records, stat points past a byte - the record after (the rolls: the generator
    # as set, 4 draws of rand(256) a level)
    ups = []
    RNG = 0x03004A5C
    for form in (0, 9, 19, 21, 26, 31, 39, 41):
        for old, new in ((50, 51), (60, 62), (99, 104), (150, 151), (198, 200)):
            for mh, me in ((135, 90), (3000, 300)):
                for pts in (0, 250):
                    for seed in ((1, 2), (0x9E3779B9, 0x12345678)):
                        e.load(st)
                        e.w8(REC + 0x14, form)
                        e.w16(REC + 0x18, mh); e.w16(REC + 0x16, 1); e.w16(REC + 0x1C, me); e.w16(REC + 0x1A, 1)
                        e.w16(REC + 0x2C, pts)
                        e.w32(RNG, seed[0]); e.w32(RNG + 4, seed[1])
                        call(e, 0x08013678, (REC, old, new, 0))
                        ups.append(dict(form=form, old=old, new=new, max_hp=mh, max_ep=me, points=pts, rng=list(seed),
                                        after=dict(hp=e.r16(REC + 0x16), max_hp=e.r16(REC + 0x18), ep=e.r16(REC + 0x1A), max_ep=e.r16(REC + 0x1C),
                                                   points=e.r16(REC + 0x2C))))
    # the level cap: the routine on Goku's record with the XP set first - just under level 200's threshold, at it
    # (level 200: the level check is skipped, the XP still counts), far past it, and at the 32-bit word's end (the add
    # wraps); `level` = the game's level function 0x080135D2 on the record after
    T = 0x086B7084
    t199, t200 = e.r32(T + 4 * 149), e.r32(T + 4 * 150)
    cap = []
    for name, x0, amount in (('199 small', t199 + 5, 7), ('199 to the threshold - 1', t200 - 8, 7), ('200 small', t200, 7),
                             ('200 big', t200 + 123, 1_000_000_000), ('200 huge', t200, 0x7FFFFFFF), ('200 wrap', 0xFFFFFF00, 0x200),
                             ('past 200 small', t200 + 5_000_000, 99_999)):
        e.load(st)
        call(e, 0x08013C66, (REC, 0, 0, 0))
        e.w32(REC + 0x24, x0)
        before = dict(hp=e.r16(REC + 0x16), max_hp=e.r16(REC + 0x18), points=e.r16(REC + 0x2C))
        call(e, 0x08013914, (REC, amount, 0, 0))
        after = dict(hp=e.r16(REC + 0x16), max_hp=e.r16(REC + 0x18), points=e.r16(REC + 0x2C))
        lv = call(e, 0x080135D2, (REC, 0, 0, 0)) & 0xFF
        cap.append(dict(name=name, xp_before=x0, xp=amount, xp_after=e.r32(REC + 0x24), level=lv, same=before == after))
        print(cap[-1])
    for c in out:
        c.pop('level', None)
        print(c)
    print(len(ups), 'level-ups', ups[0], ups[-1])
    (ROOT / 'data' / 'buu' / 'xp_routine.json').write_text(json.dumps(dict(xp=out, level_up=ups, cap=cap), separators=(',', ':')) + '\n')


if __name__ == '__main__':
    main()
