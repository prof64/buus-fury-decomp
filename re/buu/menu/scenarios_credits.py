"""Staff credits scenarios (menu/credits.rs): `run_credits` 0xD1 injected into the script runner. The skippable
variant (Options / title) is reached by clearing the mode's +0xC (unskippable) right after it starts."""
from scenarios_minigame import native, script_probe

CREDITS_VT = 0x08049338


def make_skippable(e):
    m = e.r32(0x03000F90)
    if e.r32(m) == CREDITS_VT and e.r32(m + 0xC) == 1:
        e.w32(m + 0xC, 0)
    return script_probe(e)


SCENARIOS = {
    # the first pages at normal speed (fade in, hold, fade out, the next pair)
    'cr_native': ('buu_snakeway_start.ss', None, ['-:3'], ['-:420', 'A:2', '-:20', 'B:2', '-:40'],
                  {'open': {'kind': 'credits'}, 'trigger': native([('n', 0xD1)]), 'probe': script_probe}),
    # DOWN held skips every hold: all 47 pages to the end
    'cr_native_fast': ('buu_snakeway_start.ss', None, ['-:3'], ['-:30', 'DOWN:1700', '-:30'],
                       {'open': {'kind': 'credits'}, 'trigger': native([('n', 0xD1)]), 'probe': script_probe}),
    # skippable: A quits (sound 38) after the current page's fade out
    'cr_skip': ('buu_snakeway_start.ss', None, ['-:3'], ['-:200', 'A:2', '-:80'],
                {'open': {'kind': 'credits', 'skippable': True}, 'trigger': native([('n', 0xD1)]), 'probe': make_skippable}),
}
