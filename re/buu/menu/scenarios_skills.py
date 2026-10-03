"""Skills sub-page scenarios (menu/skills.rs). Status -DOWN-> Skills."""
from menuhelp import rec


def keys(*steps):
    out = []
    for s in steps:
        out += [s + ':3', '-:16']
    return out


def setup_levels(e):
    # Energy Blast (slot 0) learned to level 2, chosen 0; a fifth skill row
    a = rec(0)
    e.w8(a + 0x2E, 0)
    e.w8(a + 0x2F, 2)


def setup_five(e):
    a = rec(0)
    e.w8(a + 0xC, 5)
    for k, s in enumerate([7, 15, 14, 2, 4]):
        e.w8(a + 0xD + k, s)
    setup_levels(e)


SCENARIOS = {
    # START from Skills closes the whole menu (Skills' done check returns 4, Status closes)
    'skills_start': ('buu_snakeway_start.ss', None, ['-:2'], ['START:1', '-:40'] + keys('DOWN', 'RIGHT', 'START') + ['-:60']),
    # Goku's 3 skills: columns, rows, description popup, B back to Status, START from Skills
    'skills_basic': ('buu_snakeway_start.ss', None, ['-:2'],
                     ['START:1', '-:40'] + keys('DOWN', 'RIGHT', 'LEFT', 'DOWN', 'UP', 'A', 'B', 'RIGHT', 'A', 'B', 'B') + ['-:60']),
    # popup level choice, L / R out of Skills, five skills (the row-follow quirk)
    'skills_levels': ('buu_snakeway_start.ss', setup_five, ['-:2'],
                      ['START:1', '-:40'] + keys('DOWN', 'A', 'RIGHT', 'RIGHT', 'RIGHT', 'LEFT', 'B', 'DOWN', 'DOWN', 'RIGHT', 'UP',
                                                 'UP', 'L') + ['-:40'] + keys('R', 'DOWN', 'R', 'START') + ['-:60']),
}
