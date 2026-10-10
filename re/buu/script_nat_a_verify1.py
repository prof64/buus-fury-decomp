"""Emulator checks of the non-blocking natives 0x00..0x37 (state buu_snakeway_start.ss).
Each test loads the base state, runs a tiny bytecode program through the game's runner and inspects RAM."""
from script_nat_a_lib import *

GD = 0x03001068
REC0 = GD + 0x4F8
e = emu('buu_snakeway_start.ss')
e.run(2)
base = e.save()
res = []


def run(items, params=0):
    e.load(base)
    r = run_code(e, asm(items), params)
    return r


def log(name, ok, detail):
    res.append((name, ok, detail))
    print(('OK  ' if ok else 'FAIL'), name, detail)


# flags
r = run([300, ('n', 0x1d), 300, ('n', 0x1b), 300, ('n', 0x1c)])
byte = e.r8(GD + 0x55 + 300 // 8)
log('set_flag/get_flag/not_flag', r[2][-2:] == [1, 0] and byte & (1 << (300 % 8)), (r[2], hex(byte)))
r = run([300, ('n', 0x1d), 300, ('n', 0x1e), 300, ('n', 0x1b)])
log('clear_flag', r[2][-1] == 0, r[2])
# globals G0..3, P0..3, L0..3 (0x03001020)
r = run([11, ('s', 0), 12, ('s', 1), 13, ('s', 2), 14, ('s', 3), ('n', 0), ('n', 1), ('n', 2), ('n', 3)])
log('G0..G3', r[2][-4:] == [11, 12, 13, 14] and e.r32(0x03001058) == 11, r[2])
r = run([21, ('s', 4), 22, ('s', 5), 23, ('s', 6), 24, ('s', 7), ('n', 4), ('n', 5), ('n', 6), ('n', 7)])
log('P0..P3', r[2][-4:] == [21, 22, 23, 24], (r[2], hex(r[0])))
r = run([31, ('s', 8), 32, ('s', 9), 33, ('s', 10), 34, ('s', 11), ('n', 8), ('n', 9), ('n', 10), ('n', 11)])
log('L0..L3 (0x03001020)', r[2][-4:] == [31, 32, 33, 34] and e.r32(0x03001020) == 31, r[2])
# HP etc
e.load(base)
cur = e.r8(GD)
rec = REC0 + 0x68 * cur
hp, mhp, ep, mep = e.r16(rec + 0x16), e.r16(rec + 0x18), e.r16(rec + 0x1a), e.r16(rec + 0x1c)
r = run([('n', 0x0c), ('n', 0x0d), ('n', 0x0e), ('n', 0x0f), ('n', 0x10), ('n', 0x15)])
log('get hp/maxhp/energy/maxep/zenie/char', r[2] == [hp, mhp, ep, mep, e.r32(GD + 0x4F4), cur],
    (r[2], hp, mhp, ep, mep))
r = run([5, ('s', 0x0c)])
log('set_hp 5', e.r16(rec + 0x16) == 5, e.r16(rec + 0x16))
r = run([9999, ('s', 0x0c)])
log('set_hp clamps to max', e.r16(rec + 0x16) == mhp, e.r16(rec + 0x16))
r = run([777, ('s', 0x0d)])
log('set_max_hp sets hp+max', (e.r16(rec + 0x16), e.r16(rec + 0x18)) == (777, 777), (e.r16(rec + 0x16), e.r16(rec + 0x18)))
r = run([3, ('s', 0x0e)])
log('set_energy', e.r16(rec + 0x1a) == 3, e.r16(rec + 0x1a))
r = run([55, ('s', 0x0f)])
log('set_max_ep sets ep+max', (e.r16(rec + 0x1a), e.r16(rec + 0x1c)) == (55, 55), (e.r16(rec + 0x1a), e.r16(rec + 0x1c)))
r = run([-5, ('s', 0x10)])
log('set_zenie clamps >=0', e.r32(GD + 0x4F4) == 0, e.r32(GD + 0x4F4))
r = run([1234, ('s', 0x10), ('n', 0x10)])
log('set_zenie', r[2][-1] == 1234, r[2])
r = run([7, ('s', 0x11), ('n', 0x11)])
log('byte 0x03002bb0', r[2][-1] == 7 and e.r8(0x03002bb0) == 7, r[2])
r = run([('n', 0x12), ('n', 0x13), ('n', 0x14)])
log('area bytes', True, (r[2], [e.r8(e.r32(0x0300197c) + k) for k in range(3)] if False else [hex(e.r32(0x0300197c + k * 4)) for k in range(2)]))
# string pointers
r = run([('n', 0x16), ('n', 0x17)])
from script_nat_a_str import ustr_at
log('form name/desc string ptrs', True, [ustr_at(p) for p in r[2]])
# rand / chance
r = run([100, ('n', 0x19), 100, ('n', 0x19), 0, ('n', 0x1a), 100, ('n', 0x1a)])
log('rand/chance', 0 <= r[2][0] < 100 and r[2][2] == 0 and r[2][3] == 1, r[2])
# sfx
VO = 0x03003898 + 0x5C4
r = run([20, ('n', 0x1f)])
act = [(k, hex(e.r32(VO + 0x40 * k + 0x28)), e.r8(VO + 0x40 * k + 0x12)) for k in range(16) if e.r8(VO + 0x40 * k + 0x10) & 1]
log('play_sfx 20', any(a[1] == hex(0x083192A8 + 12 * 20) for a in act), act)
r = run([20, 33, ('n', 0x20)])
act = [(k, hex(e.r32(VO + 0x40 * k + 0x28)), e.r8(VO + 0x40 * k + 0x12)) for k in range(16) if e.r8(VO + 0x40 * k + 0x10) & 1]
log('play_sfx_vol 20,33', any(a[1] == hex(0x083192A8 + 12 * 20) and a[2] == 33 for a in act), act)
r = run([20, ('n', 0x1f), 20, ('n', 0x21)])
act = [(k, hex(e.r32(VO + 0x40 * k + 0x28))) for k in range(16) if e.r8(VO + 0x40 * k + 0x10) & 1]
log('stop_sfx 20', not any(a[1] == hex(0x083192A8 + 12 * 20) for a in act), act)
# music
PL = 0x030038A4
r = run([7, ('n', 0x23)])
log('play_music 7', e.r32(PL + 0xC) == 0x083BB78C + 20 * 7 and e.r8(PL + 5) == 1, (hex(e.r32(PL + 0xC)), e.r8(PL + 5)))
r = run([7, ('n', 0x23), ('n', 0x24)])
log('stop_music', e.r8(PL + 5) == 0, e.r8(PL + 5))
# display
d0 = e.r16(0x04000000)
r = run([3, ('n', 0x26)])
log('hide_bg 3', not (e.r16(0x04000000) & 0x800), hex(e.r16(0x04000000)))
r = run([3, ('n', 0x26), 3, ('n', 0x25)])
log('show_bg 3', bool(e.r16(0x04000000) & 0x800), hex(e.r16(0x04000000)))
r = run([1, 2, ('n', 0x27)])
log('set_bg_priority 1,2', True, hex(e.r16(0x0400000A)))
r = run([1, 3, ('n', 0x28)])
log('set_blend_layer 1 both', True, hex(e.r16(0x04000050)))
r = run([50, 25, ('n', 0x2a)])
log('set_blend_alpha 50,25', True, hex(e.r16(0x04000052)))
# player id lookup / count
r = run([0, ('n', 0x37), -1, ('n', 0x37)])
log('count_actors(0)', True, r[2])
for name, ok, d in res:
    pass
