"""Switch Character scenarios (menu/charsel.rs): `character_menu` 0x8D injected right before frame 0. The figures'
idle animations start at random ticks (rand(255) each): meta "open" gets the RNG state (0x03004A5C / 60) of the end
of the last frame before the screen's ctor (`0x0802E83C`; the world is frozen in between)."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from menuhelp import setup_party, chain, rec
from scenarios_shop import inject

S = 'buu_snakeway_start.ss'
CHARSEL_VT = 0x0804A5E0


def charsel():
    meta = {'open': {'kind': 'charsel'}, 'trigger': inject([('n', 0x8D)])}

    def probe(e):
        return dict(rng=[e.r32(0x03004A5C), e.r32(0x03004A60)], cs=int(e.r32(e.r32(0x03000F90)) == CHARSEL_VT))

    def finish(frames, extra):
        for k, f in enumerate(frames):
            if f.get('cs'):
                extra['open']['rng'] = frames[k - 1]['rng']
                break
        for f in frames:
            f.pop('rng', None)
            f.pop('cs', None)
    meta['probe'] = probe
    meta['finish'] = finish
    return meta


def no_flag2(chars):
    """party members that cannot be chosen (character flag bit 1 clear: silhouettes)"""
    def f(e):
        for c in chars:
            e.w8(rec(c), e.r8(rec(c)) & ~2)
    return f


SCENARIOS = {
    # Goku alone: look, leave with B
    'charsel_alone': (S, None, ['-:3'], ['-:60', 'RIGHT:2', '-:20', 'B:2', '-:70'], charsel()),
    # five in the party: move right / left, pick Vegeta
    'charsel_pick': (S, setup_party([1, 2, 3, 4]), ['-:3'],
                     ['-:60', 'RIGHT:2', '-:20', 'RIGHT:2', '-:20', 'LEFT:2', '-:20', 'RIGHT:2', '-:20', 'RIGHT:2', '-:20',
                      'RIGHT:2', '-:20', 'A:2', '-:90'], charsel()),
    # Gohan and Goten in the party but not choosable (silhouettes, skipped by LEFT / RIGHT); a long look (idle blinks)
    'charsel_locked': (S, chain(setup_party([1, 2, 4]), no_flag2([1, 2])), ['-:3'],
                       ['-:60', 'RIGHT:2', '-:30', 'RIGHT:2', '-:30', 'LEFT:2', '-:200', 'B:2', '-:70'], charsel()),
}
