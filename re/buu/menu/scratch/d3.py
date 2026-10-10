p = '/Users/prof/source/legacy-of-goku-x/.claude/worktrees/re-dashboard/re/buu/menu/scenarios_minigame.py'
s = open(p).read()
a = "SCENARIOS = {"
b = '''from script_nat_b_lib import INTERP_AT
from emutools import MAILBOX


def script_probe(e):
    """the injected script's stack (the natives' pushes) once it ended"""
    done = e.r32(MAILBOX) != 0xDEADBEEF
    sp = e.r32(INTERP_AT)
    return dict(done=done, stack=[e.r32(INTERP_AT + 4 + 4 * k) for k in range(min(sp, 4))] if done else [])


def watch_probe(e):
    """Watch This!'s game object (mode +4): the five buttons (+0x24) and the script's stack"""
    d = script_probe(e)
    m = e.r32(0x03000F90)
    g = e.r32(m + 4)
    if 0x02000000 <= g < 0x02040000 and e.r32(m) == 0x0804A3E8:
        d['seq'] = [e.r8(g + 0x24 + k) for k in range(5)]
    return d


def finish_result(frames, extra):
    """the native's push (stack top) after the script ended; Watch This: the sequence it drew"""
    for f in frames:
        if f.get('seq') and 'seq' not in extra['open']:
            extra['open']['seq'] = f['seq']
    for f in frames:
        if f.get('done'):
            st = [v - (1 << 32) if v & 0x80000000 else v for v in f['stack']]
            extra['result'] = st[-1] if st else None
            break


def presses(seq, gap=6):
    names = ['A', 'B', 'RIGHT', 'LEFT', 'UP', 'DOWN']
    out = []
    for b in seq:
        out += [names[b] + ':2', '-:%d' % gap]
    return out


PROBE = {'probe': script_probe, 'finish': finish_result}
WPROBE = {'probe': watch_probe, 'finish': finish_result}

SCENARIOS = {'''
s = s.replace(a, b, 1)
s = s.replace("""'trigger': native([('n', 0xC6)])}""", """'trigger': native([('n', 0xC6)]), **PROBE}""")
s = s.replace("""'trigger': native([1, ('n', 0xC7)])}""", """'trigger': native([1, ('n', 0xC7)]), **PROBE}""")
s = s.replace("""'trigger': native([3, ('n', 0xC7)])}""", """'trigger': native([3, ('n', 0xC7)]), **PROBE}""")
s = s.rstrip()
assert s.endswith('}')
s = s[:-1] + '''    # Watch This! level 1 (21 frames per icon): the drawn sequence (WATCH_SEQ1: this state's RNG), the fixed code,
    # wrong presses with the SELECT / START quirk at level 7 (3 frames per icon)
    'mg_watch_ok': ('buu_snakeway_start.ss', None, ['-:3'], ['-:240'] + presses(WATCH_SEQ1) + ['-:100'], {'open': {'kind': 'watch_this', 'level': 1}, 'trigger': native([1, ('n', 0xA7)]), **WPROBE}),
    'mg_watch_code': ('buu_snakeway_start.ss', None, ['-:3'], ['-:240'] + presses([4, 1, 3, 5, 0]) + ['-:100'], {'open': {'kind': 'watch_this', 'level': 1}, 'trigger': native([1, ('n', 0xA7)]), **WPROBE}),
    'mg_watch_bad': ('buu_snakeway_start.ss', None, ['-:3'], ['-:60', 'START+RIGHT:2', '-:4', 'L+A:2', '-:4', 'DOWN:1', '-:1', 'UP:1', '-:1', 'SELECT+B:2', '-:80'], {'open': {'kind': 'watch_this', 'level': 7}, 'trigger': native([7, ('n', 0xA7)]), **WPROBE}),
}
'''
s = s.replace("MASH = ", "WATCH_SEQ1 = [0, 0, 0, 0, 0]\nMASH = ", 1)
open(p, 'w').write(s)
print('ok')
