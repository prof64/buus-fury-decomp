"""Options page scenarios (menu/options.rs). Status -L-> Journal -L-> Options."""

P = ['-:16']


def keys(*steps):
    out = []
    for s in steps:
        out += [s + ':3', '-:16']
    return out


SCENARIOS = {
    # sliders (SFX / music / text speed), the control cycle, Credits <-> Sleep, B closes
    'options_sliders': ('buu_snakeway_start.ss', None, ['-:2'],
                        ['START:1', '-:40'] + keys('L', 'L', 'RIGHT', 'RIGHT', 'DOWN', 'LEFT', 'DOWN', 'LEFT', 'LEFT', 'RIGHT',
                                                   'DOWN', 'RIGHT', 'LEFT', 'UP', 'UP', 'UP', 'DOWN', 'B') + ['-:60']),
    # Sleep: A closes the menu, the field runs event s007 (text box, sleep_mode)
    'options_sleep': ('buu_snakeway_start.ss', None, ['-:2'],
                      ['START:1', '-:40'] + keys('L', 'L', 'UP', 'RIGHT', 'A') + ['-:80']),
    # L / R around Options and START closing it
    'options_pages': ('buu_snakeway_start.ss', None, ['-:2'],
                      ['START:1', '-:40'] + keys('L', 'L', 'R', 'L', 'START') + ['-:60']),
}
