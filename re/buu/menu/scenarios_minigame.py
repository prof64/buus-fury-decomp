"""Mini-game scenarios (menu/minigame.rs): the natives are injected into the game's script runner
(re/buu/script_nat_b_lib.py: the main loop's BL is hooked for one call, the bytecode runs until it blocks)."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


def native(items):
    """trigger: run bytecode `items` (script_nat_b_lib.asm format) through the game's runner 0x08004098; the call
    starts in the next frame's main loop and keeps running (blocking natives run their nested modes)"""
    def f(e):
        from script_nat_b_lib import asm, CODE_AT, HDR_AT, INTERP_AT, start_call
        code = asm(items)
        blob = b'\x01\x00\x00' + code
        blob += b'\x00' * (-len(blob) % 4)
        for i in range(0, len(blob), 4):
            e.w32(CODE_AT + i, int.from_bytes(blob[i:i + 4], 'little'))
        e.w32(HDR_AT, CODE_AT)
        for i in range(0, 0x6c, 4):
            e.w32(INTERP_AT + i, 0)
        start_call(e, 0x08004098, (INTERP_AT, HDR_AT, 0, 0))
    return f


WATCH_SEQ1 = [5, 4, 2, 3, 3]
MASH = ['-:30', 'A:2', '-:2', 'A:2', '-:2', 'A:2', '-:2', 'A:2', '-:2', 'A:2', '-:2', 'A:2', '-:2', '-:20', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', 'A:1', '-:1', 'B:1', '-:1', '-:60']

from script_nat_b_lib import INTERP_AT
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
    if frames and extra['open'].get('kind') == 'punch':
        # the boot frame counter at frame 0 (the punch marker blinks with it)
        extra['open']['counter'] = frames[0]['lf']
    for f in frames:
        if f.get('seq') and 'seq' not in extra['open']:
            extra['open']['seq'] = f['seq']
    for f in frames:
        if f.get('done'):
            st = [v - (1 << 32) if v & 0x80000000 else v for v in f['stack']]
            extra['native_push'] = st[-1] if st else None
            break


def presses(seq, gap=6):
    names = ['A', 'B', 'RIGHT', 'LEFT', 'UP', 'DOWN']
    out = []
    for b in seq:
        out += [names[b] + ':2', '-:%d' % gap]
    return out


PROBE = {'probe': script_probe, 'finish': finish_result}
WPROBE = {'probe': watch_probe, 'finish': finish_result}

SCENARIOS = {
    # power struggle (0xC6): a few presses, the decay, then fast mashing to the top, the 30-frame wait, the end
    'mg_power': ('buu_snakeway_start.ss', None, ['-:3'], MASH, {'open': {'kind': 'power_struggle'}, 'trigger': native([('n', 0xC6)]), **PROBE}),
    # punch (0xC7) at speed 1 and 3: stop the marker with A, the blink, the result
    'mg_punch1': ('buu_snakeway_start.ss', None, ['-:3'], ['-:77', 'A:2', '-:60'], {'open': {'kind': 'punch', 'mode': 1}, 'trigger': native([1, ('n', 0xC7)]), **PROBE}),
    'mg_punch3': ('buu_snakeway_start.ss', None, ['-:3'], ['-:150', 'A:1', '-:61'], {'open': {'kind': 'punch', 'mode': 3}, 'trigger': native([3, ('n', 0xC7)]), **PROBE}),
    # Watch This! level 1 (21 frames per icon): the drawn sequence (WATCH_SEQ1: this state's RNG), the fixed code,
    # wrong presses with the SELECT / START quirk at level 7 (3 frames per icon)
    'mg_watch_ok': ('buu_snakeway_start.ss', None, ['-:3'], ['-:240'] + presses(WATCH_SEQ1) + ['-:100'], {'open': {'kind': 'watch_this', 'level': 1}, 'trigger': native([1, ('n', 0xA7)]), **WPROBE}),
    'mg_watch_code': ('buu_snakeway_start.ss', None, ['-:3'], ['-:240'] + presses([4, 1, 3, 5, 0]) + ['-:100'], {'open': {'kind': 'watch_this', 'level': 1}, 'trigger': native([1, ('n', 0xA7)]), **WPROBE}),
    'mg_watch_bad': ('buu_snakeway_start.ss', None, ['-:3'], ['-:60', 'START+RIGHT:2', '-:4', 'L+A:2', '-:4', 'DOWN:1', '-:1', 'UP:1', '-:1', 'SELECT+B:2', '-:80'], {'open': {'kind': 'watch_this', 'level': 7}, 'trigger': native([7, ('n', 0xA7)]), **WPROBE}),
}
