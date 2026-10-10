"""trade_record.py NAME --a PICKS --b PICKS: record a Trading Post trade between two linked emulators (tools/emu shim
`emu_link`, mGBA's lockstep coordinator, one process, headless and muted).

Both games boot with run 2's in-game save (work/run2/end.sav), go title -> Multiplayer -> Trading Post -> the link ->
game 1. Each side then puts its picks on the table ("Your Trade:"): PICKS = comma-separated DOWN[xCOUNT] - DOWN
presses from where the cursor is (it stays put after a pick; a picked entry leaves the list), A, and COUNT for the
"How many to trade?" prompt (RIGHT raises it from 1). The offers as the screen holds them are recorded at the
proposal (`offer`: each side's as the other's "Friend's Trade:" holds it). Then R ("Propose
Trade") on both, A ("ACCEPT") on both, "TRADE COMPLETE!", and the save is written. Writes data/link/trade_NAME.json:
the picks, and for each side game 1's tradable part of the save (the 83 supply counts, the equipment list's raw
words, zenie, the characters' equipment slots) before and after (the save is EEPROM, 8-byte blocks byte-reversed; slot 0 at 0x10, 0x8A8 bytes)."""
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from dojo_record import L, ROOT, mkemu, soft_reset, sbemu, keyof, step  # noqa: E402

# title -> Multiplayer -> the cable's wait -> Trading Post -> game 1 -> the picker
PRE = [('-', '-', 1100), ('A', 'A', 7), ('-', '-', 40), ('DOWN', 'DOWN', 7), ('-', '-', 30), ('A', 'A', 7),
       ('-', '-', 300), ('DOWN', 'DOWN', 7), ('-', '-', 30), ('A', 'A', 7), ('-', '-', 120), ('A', 'A', 7),
       ('-', '-', 240)]


def slot0(e):
    """game 1's tradable part: supplies (83 counts), equipment words, zenie"""
    raw = bytes(sbemu.sd_read(e))
    b = b''.join(raw[i:i + 8][::-1] for i in range(0, len(raw), 8))
    gd = b[0x10:0x10 + 0x8A8]
    n = min(struct.unpack_from('<I', gd, 0xE4)[0], 250)
    # (and each character's equipment slots: record c at 0x4F8 + 0x68 c, +0x3C)
    return {'supplies': list(gd[2:2 + 83]), 'equipment': [struct.unpack_from('<I', gd, 0xE8 + 4 * k)[0] for k in range(n)],
            'zenie': struct.unpack_from('<I', gd, 0x4F4)[0],
            'worn': [[struct.unpack_from('<I', gd, 0x4F8 + 0x68 * c + 0x3C + 4 * k)[0] for k in range(4)] for c in range(9)]}


def offer(e, at):
    """the friend's offer as the trade screen holds it (EWRAM 0x0203CDA4, found by diffing two proposals): a count,
    then 8-byte entries - amount (u32), kind (0 equipment / 1 supply / 2 zenie), id"""
    n = min(e.r32(at), 8)
    return [[e.r32(at + 4 + 8 * k), e.r8(at + 8 + 8 * k), e.r8(at + 9 + 8 * k)] for k in range(n)]


FRIENDS = 0x0203CDA4


def picks_keys(picks):
    out = []
    for p in picks:
        idx, cnt = (int(x) for x in (p.split('x') + ['1'])[:2])
        out += [('DOWN', 7), ('-', 20)] * idx + [('A', 7), ('-', 40)]
        if cnt > 1 or 'x' in p:
            out += [('RIGHT', 7), ('-', 20)] * (cnt - 1) + [('A', 7), ('-', 40)]
    return out


def main():
    name = sys.argv[1]
    arg = lambda k, d: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else d
    pa, pb = [p for p in arg('--a', '').split(',') if p], [p for p in arg('--b', '').split(',') if p]
    sav = (ROOT / 'work' / 'run2' / 'end.sav').read_bytes()
    a, b = mkemu('buu_menus.ss'), mkemu('buu_menus.ss')
    for e in (a, b):
        sbemu.sd_write(e, sav)
        e.run(3)
        soft_reset(e)
    before = [slot0(a), slot0(b)]
    L.emu_link(a.h, 0)
    L.emu_link(b.h, 1)

    def run(seq):
        for ka, kb, n in seq:
            for _ in range(n):
                a.set_keys(keyof(ka))
                b.set_keys(keyof(kb))
                step(a, b)

    run(PRE)
    # (one side's picks while the other waits)
    run([(k, '-', n) for k, n in picks_keys(pa)])
    run([('-', k, n) for k, n in picks_keys(pb)])
    run([('R', 'R', 7), ('-', '-', 60)])
    # (each side's offer as the other received it, "Friend's Trade:" - what the apply step adds)
    offers = [offer(b, FRIENDS), offer(a, FRIENDS)]
    if '--shots' in sys.argv:
        # (the proposal both sides accept: "Your Trade:" / "Friend's Trade:")
        a.screenshot(Path(arg('--shots', '/tmp')) / f'{name}_a.png', 1)
        b.screenshot(Path(arg('--shots', '/tmp')) / f'{name}_b.png', 1)
    run([('A', 'A', 7), ('-', '-', 900)])
    after = [slot0(a), slot0(b)]
    out = ROOT / 'data' / 'link' / f'trade_{name}.json'
    out.write_text(json.dumps({'name': name, 'about': __doc__.split('\n')[0], 'picks': [pa, pb], 'offers': offers, 'before': before,
                               'after': after}, indent=None) + '\n')
    print(out, 'zenie', [s['zenie'] for s in before], '->', [s['zenie'] for s in after])


if __name__ == '__main__':
    main()
