"""Journal page scenarios (menu/journal.rs). Status -L-> Journal."""
from menuhelp import setup_flags


def keys(*steps):
    out = []
    for s in steps:
        out += [s + ':3', '-:16']
    return out


# 8 goals (quests 1, 2, 6-11) and 4 completed (0, 3, 4, 5)
FLAGS = [0x02, 0x03, 0x0C, 0x0F, 0x15, 0x19, 0x1B, 0x23, 0x27]

SCENARIOS = {
    'journal_start': ('buu_snakeway_start.ss', setup_flags(FLAGS), ['-:2'], ['START:1', '-:40'] + keys('L', 'RIGHT', 'START') + ['-:60']),
    'journal_new_game': ('buu_snakeway_start.ss', None, ['-:2'],
                         ['START:1', '-:40'] + keys('L', 'RIGHT', 'LEFT', 'B') + ['-:60']),
    'journal_scroll': ('buu_snakeway_start.ss', setup_flags(FLAGS), ['-:2'],
                       ['START:1', '-:40'] + keys('L', 'DOWN', 'DOWN', 'DOWN', 'DOWN', 'DOWN', 'DOWN', 'UP', 'UP', 'RIGHT', 'DOWN',
                                                  'LEFT', 'L', 'R', 'B') + ['-:60']),
}
