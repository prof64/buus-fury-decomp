"""Switch Character scenarios (menu/charsel.rs): `character_menu` 0x8D injected right before frame 0."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from menuhelp import setup_party
from scenarios_shop import inject

S = 'buu_snakeway_start.ss'


def charsel():
    return dict(open={'kind': 'charsel'}, trigger=inject([('n', 0x8D)]))


SCENARIOS = {
    # Goku alone: look, leave with B
    'charsel_alone': (S, None, ['-:3'], ['-:60', 'RIGHT:2', '-:20', 'B:2', '-:70'], charsel()),
    # five in the party: move right / left, pick Vegeta
    'charsel_pick': (S, setup_party([1, 2, 3, 4]), ['-:3'],
                     ['-:60', 'RIGHT:2', '-:20', 'RIGHT:2', '-:20', 'LEFT:2', '-:20', 'RIGHT:2', '-:20', 'RIGHT:2', '-:20',
                      'RIGHT:2', '-:20', 'A:2', '-:90'], charsel()),
}
