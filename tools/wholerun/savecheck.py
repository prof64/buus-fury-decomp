"""savecheck.py CUT.ss.. [--out OUT.json]: the game's own save -> quit -> Continue from whole-run cut states (the
in-game save files check; examples/savecheck.rs does the same on the app and compares).

For each state: the game's Save Game routine 0x08004962(GameData, slot 0, &[x, y] (the leader's px position), flag
0) - the location bytes, the "game exists" bit, the slot record and its EEPROM write, as the save menu calls it - then
a soft reset, the title, Single Player, Game 1 (Continue: the game's boot + slot load path), until the leader is
controllable in the field. Written per state: the slot's GameData bytes (hex) as saved, and after the Continue: area,
leader position, the GameData (0x8A8 bytes, hex), the RNG words."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
for d in ('tools/superbuu', 'tools/story_playthrough', 'tools/field', 'tools/segments', 're/buu'):
    sys.path.insert(0, str(ROOT / d))
from sbemu import sd_read  # noqa: E402  (the savedata-capable emulator lib first)
from common import emu  # noqa: E402
from flashlib import call_long  # noqa: E402
from route import status  # noqa: E402
from mksave import title_continue  # noqa: E402

GD = 0x03001068
SCRATCH = 0x0203FF00   # 8 bytes of EWRAM's top for the position words


def title_continue_first(e, log):
    """mksave.title_continue up to the first frame the leader is controllable (no event, a player object) - the
    state compared (mksave's version runs 30 more frames, in which enemies may already act)"""
    from common import KEY

    def k(keys, n):
        for _ in range(n):
            e.set_keys(keys)
            e.run(1)
            log.append(keys)
    k(KEY.A | KEY.B | KEY.START | KEY.SELECT, 10)
    k(0, 5)
    for _ in range(30):
        k(KEY.START, 4)
        k(0, 36)
        if status(e)['p'] is not None:
            break
    for i in range(2000):
        st = status(e)
        if st['p'] is not None and st['scene'] == 0:
            break
        k(KEY.A if i % 20 < 3 else 0, 1)


READY = ROOT / 'work' / 'wholerun' / 'savecheck_cuts'
PHASE = ('import sys; from pathlib import Path; R = Path(%r); '
         'sys.path.insert(0, str(R / "tools/story_playthrough")); sys.path.insert(0, str(R / "tools/field")); '
         'from route import start_emu; e = start_emu(sys.argv[1]); c0, p0 = e.cycles(), e.r32(0x0300389C)\n'
         'for _ in range(200000):\n'
         '    e.step()\n'
         '    if e.r32(0x0300389C) != p0: print(e.cycles() - c0); break') % str(ROOT)


def ready_cut(e, name):
    """the save's moment as an importable cut: the end of a game iteration with the leader in the field (a cut state
    can stand inside a long iteration - an area load, no leader yet: whole-run cut 631,386 in 8-3, which the importer
    rightly refuses; the save menu is never open there). Runs on (no keys) to the first iteration end with a leader
    and no event, saves that state and records a short doc from it (`segment.record`, rows to two iteration ends)."""
    import segment as sg
    from route import start_emu
    lf0 = e.r32(0x03000F94)
    for _ in range(1200):
        st = status(e)
        # the iteration ended: the counter moved since the last check and the next frame starts a new one
        if st['p'] is not None and st['scene'] == 0 and e.r32(0x03000F94) != lf0:
            break
        e.set_keys(0)
        e.run(1)
    READY.mkdir(parents=True, exist_ok=True)
    ss = READY / f'{name}.ss'
    e.save_file(str(ss))
    e2 = start_emu(str(ss))
    init, objs, rows = sg.record(e2, lambda j, _e: 0, 64)
    ends = [k for k in range(len(rows) - 1) if rows[k + 1]['lf'] != rows[k]['lf']]
    rows = rows[:ends[1] + 1] if len(ends) >= 2 else rows
    # cycles to the next sound-DMA interrupt (tools/segments/snd_phase.py's measure; a separate process - the
    # savedata emulator library loaded here lacks the cycle counter)
    import subprocess
    init['snd_phase'] = int(subprocess.run([sys.executable, '-c', PHASE, str(ss)], capture_output=True, text=True, check=True).stdout)
    doc = {'kind': 'segment', 'name': f'save_{name}', 'source': {'kind': 'state', 'state': str(ss)}, 'init': init, 'objs': objs, 'frames': rows}
    (READY / f'{name}.json').write_text(json.dumps(doc, separators=(',', ':')))
    return emu(str(ss)), str(ss)


def one(st):
    e = emu(str(Path(st).resolve()))
    e, st = ready_cut(e, Path(st).stem)
    s0 = status(e)
    if s0['p'] is None:
        return {'state': str(Path(st).resolve()), 'skip': 'no leader'}
    x, y = s0['p'][0], s0['p'][1]
    e.w32(SCRATCH, x)
    e.w32(SCRATCH + 4, y)
    call_long(e, 0x08004962, (GD, 0, SCRATCH, 0))
    # the slot as written to the EEPROM (bytes 0x10.., each 8-byte block reversed), not GameData in RAM: the hook
    # (`call_long`) runs the rest of the game iteration after the save, and an area that drains HP has changed RAM by
    # then (whole-run cut 787,315: 454 in RAM, 457 saved)
    ee = sd_read(e)
    saved = b''.join(ee[k:k + 8][::-1] for k in range(0, len(ee), 8))[0x10:0x10 + 0x8A8].hex()
    log = []
    title_continue_first(e, log)
    s1 = status(e)
    return {'state': str(Path(st).resolve()), 'before': s0, 'saved_gd': saved, 'after': s1, 'gd': e.read(GD, 0x8A8).hex(),
            'rng': [e.r32(0x03004A5C), e.r32(0x03004A60)], 'continue_frames': len(log)}


def gd_json(gd, p, area):
    """the GameData image in examples/savecheck.rs's layout"""
    u = lambda o, n=4: int.from_bytes(gd[o:o + n], 'little')
    s16 = lambda o: int.from_bytes(gd[o:o + 2], 'little', signed=True)
    n = min(u(0xE4), 250)
    eq = sorted(u(0xE8 + 4 * k) & 0x0FFFFFFF for k in range(n) if u(0xE8 + 4 * k) >> 28 == 0)
    chars = []
    for c in range(9):
        a = 0x4F8 + 0x68 * c
        k = min(gd[a + 0xC], 16)
        chars.append([u(a + 0x24), s16(a + 0x16), s16(a + 0x18), s16(a + 0x1A), s16(a + 0x1C), gd[a + 0x28], gd[a + 0x29],
                      gd[a + 0x2A], gd[a + 0x2B], u(a + 0x2C, 2), gd[a + 0x14], gd[a + 0x15], list(gd[a + 0xD:a + 0xD + k])])
    return {'area': area, 'p': p, 'current': gd[0], 'zenie': u(0x4F4), 'supplies': list(gd[2:2 + 83]),
            'flags': gd[0x55:0x55 + 0x8F].hex(), 'equipment': eq, 'chars': chars}


def compare(game_json, app_json):
    game = {g['state']: g for g in json.loads(Path(game_json).read_text())}
    app = json.loads(Path(app_json).read_text())
    bad = 0
    for a in app:
        g = game.get(a['state'])
        if g is None or 'skip' in a or 'skip' in g:
            print(Path(a['state']).name, 'skipped', a.get('skip') or (g or {}).get('skip'))
            continue
        gb = gd_json(bytes.fromhex(g['saved_gd']), g['before']['p'], g['before']['area'])
        ga = gd_json(bytes.fromhex(g['gd']), g['after']['p'], g['after']['area'])
        diffs = []
        for tag, gx, ax in (('saved', gb, a['before']), ('continued', ga, a['after'])):
            for k in ('area', 'p', 'current', 'zenie', 'supplies', 'flags', 'equipment', 'chars'):
                if k == 'p' and tag == 'saved':
                    continue
                gv, av = gx[k], ax[k]
                if k == 'chars':
                    # (characters the game has no record of: never in the party)
                    gv = [c for c in gv if c[2] or c[0]]
                    av = [c for c, g0 in zip(av, gx['chars']) if g0[2] or g0[0]]
                if gv != av:
                    diffs.append(f'{tag} {k}: app {str(av)[:160]} game {str(gv)[:160]}')
        bad += bool(diffs)
        print(Path(a['state']).name, 'equal' if not diffs else 'DIFFERS', f"(continue: app {a['continue_steps']} steps, game {g['continue_frames']} frames)")
        for d in diffs:
            print('   ', d)
    print(len(app), 'states,', bad, 'differ')


if __name__ == '__main__':
    a = sys.argv[1:]
    if '--compare' in a:
        compare(a[a.index('--compare') + 1], a[a.index('--compare') + 2])
        sys.exit(0)
    out = Path(a[a.index('--out') + 1]) if '--out' in a else ROOT / 'work' / 'wholerun' / 'savecheck.json'
    res = []
    for p in [x for x in a if x.endswith('.ss')]:
        r = one(p)
        print(Path(p).name, r.get('skip') or (r['before']['area'], r['before']['p'], '->', r['after']['area'], r['after']['p']), flush=True)
        res.append(r)
    out.write_text(json.dumps(res))
