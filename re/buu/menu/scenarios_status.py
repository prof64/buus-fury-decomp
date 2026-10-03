"""Status page scenarios (menu/status.rs)."""
from menuhelp import setup_points, setup_party

SCENARIOS = {
    # open the pause menu, look at Status, close with B
    'status_open_close': ('buu_snakeway_start.ss', None, ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    # R / L page switches from Status, back, START closes
    'status_pages': ('buu_snakeway_start.ss', None, ['-:2'], ['START:1', '-:40', 'R:1', '-:30', 'L:1', '-:30', 'L:1', '-:30', 'R:1', '-:30', 'START:1', '-:60']),
    # stat points: the blinking LevelUp!, Use Points (A), move points, accept / cancel
    'status_alloc': ('buu_snakeway_start.ss', setup_points(5), ['-:2'],
                     ['START:1', '-:36', 'A:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'DOWN:2', '-:12', 'RIGHT:2', '-:12',
                      'LEFT:2', '-:12', 'UP:2', '-:12', 'A:2', '-:30', 'A:2', '-:12', 'RIGHT:2', '-:12', 'B:2', '-:30', 'B:2', '-:60']),
    # party tabs: Goku + Gohan + Goten + Trunks + Vegeta in the party, LEFT / RIGHT through them
    'status_tabs': ('buu_snakeway_start.ss', setup_party([1, 2, 3, 4]), ['-:2'],
                    ['START:1', '-:40', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12',
                     'LEFT:2', '-:12', 'B:2', '-:60']),
}
