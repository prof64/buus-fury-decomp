"""Save Game scenarios (menu/save.rs): `save_menu(flag)` 0x8C injected right before frame 0."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from scenarios_shop import inject

S = 'buu_snakeway_start.ss'


def save(flag=0):
    return dict(open={'kind': 'save', 'flag': flag}, trigger=inject([flag, ('n', 0x8C)]))


SCENARIOS = {
    # look at the slots, move, cancel
    'save_cancel': (S, None, ['-:3'], ['-:60', 'DOWN:2', '-:20', 'DOWN:2', '-:20', 'UP:2', '-:20', 'B:2', '-:70'], save()),
    # save into slot 2
    'save_slot': (S, None, ['-:3'], ['-:60', 'DOWN:2', '-:20', 'A:2', '-:200'], save()),
}
