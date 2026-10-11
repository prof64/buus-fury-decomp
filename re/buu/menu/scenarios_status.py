"""Status page scenarios (menu/status.rs)."""
from menuhelp import setup_points, setup_party, chain, rec

XP200 = 50410227  # the XP table's last entry (0x086B7084 + 4 * 150)

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
    # the allocation's limit: a stat stops at the level (60): STR 59 takes one point, POW 60 none; accept
    'status_alloc_cap': ('buu_snakeway_start.ss', chain(setup_points(5), lambda e: (e.w8(rec(0) + 0x28, 59), e.w8(rec(0) + 0x29, 60))), ['-:2'],
                         ['START:1', '-:36', 'A:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'RIGHT:2', '-:12', 'DOWN:2', '-:12',
                          'RIGHT:2', '-:12', 'LEFT:2', '-:12', 'DOWN:2', '-:12', 'RIGHT:20', '-:12', 'LEFT:30', '-:12', 'A:2', '-:30', 'B:2', '-:60']),
    # the last level: XP at level 200's threshold (NXT LVL), one under it (level 199), and far past it
    'status_level_200': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, XP200), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    'status_level_199': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, XP200 - 1), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    'status_level_past': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, XP200 + 123456789), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    # ... and at the 32-bit word's ends: the signed boundary, its neighbours, the last values before the wrap
    'status_xp_7fffffff': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, 0x7FFFFFFF), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    'status_xp_80000000': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, 0x80000000), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    'status_xp_fffffff0': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, 0xFFFFFFF0), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
    'status_xp_ffffffff': ('buu_snakeway_start.ss', lambda e: e.w32(rec(0) + 0x24, 0xFFFFFFFF), ['-:2'], ['START:1', '-:50', 'B:1', '-:60']),
}
