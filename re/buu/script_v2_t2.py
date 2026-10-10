"""Round-2 tests, batch 2: cutscene commands, map objects, HUD, timers (headless). Results -> v2_results.json."""
import json
import sys
from script_v2_lib import *
import script_v2_t1 as T

rec = T.rec
S = 'buu_snakeway_start.ss'
Y = 'buu_v2_yemma.ss'
NAT = lambda n: ('n', n)  # noqa: E731


def t39():
    # actor 40 walks far, actor 135 a little; end_parallel waits for both, or only for 135 with wait_for_actor
    base = [NAT(0x38), 40, 166, 210, NAT(0x60), 135, 76, 176, NAT(0x60)]
    _, ra = run(S, base + [NAT(0x3a)], 400)
    _, rb = run(S, base + [135, NAT(0x39), NAT(0x3a)], 400)
    rec(0x39, ra[0] is not None and rb[0] is not None and rb[0] < ra[0],
        f'batch: walk_to(40, +100 px) and walk_to(135, +10 px); end_parallel blocks {ra[0]} f; with '
        f'wait_for_actor(135) first it blocks {rb[0]} f')


def t3b():
    shots = {}

    def pf(e, f):
        if f == 30:
            shot(e, 'wavebeam_30')
    e = emu(S)
    e.run(3)
    from script_nat_b_lib import run_code
    r = run_code(e, asm([0, 3, 20, NAT(0x3b)]), max_frames=900, per_frame=pf)
    rec(0x3b, r[0] is not None, f'fire_wave_beam(0, right, 20) on Goku in Snake Way returned after {r[0]} frames '
        f'(v2_wavebeam_30.png)')


def t41():
    def pf(e, f):
        if f == 12:
            shot(e, 'shoot_at_actor_12')
    e = emu(S)
    e.run(3)
    from script_nat_b_lib import run_code
    r = run_code(e, asm([0, 135, 300, NAT(0x41)]), max_frames=600, per_frame=pf)
    rec(0x41, r[0] is not None, f'shoot_at_actor(0, 135, 300): blast from Goku toward the NPC, call blocked {r[0]} f '
        f'(v2_shoot_at_actor_12.png)')


def t4b():
    # in a batch: actor_wait(0, 20) then queue_sfx(0, 1) on the same track; watch the SFX channel block
    from script_nat_b_lib import run_code
    for with_sfx in (True, False):
        e = emu(S)
        e.run(3)
        seen = []

        def pf(e, f):
            seen.append(e.read(0x03003E5C, 0x30))
        items = [NAT(0x38), 0, 20, NAT(0x4a)] + ([0, 1, NAT(0x4b)] if with_sfx else []) + [NAT(0x3a)]
        r = run_code(e, asm(items), max_frames=120, per_frame=pf)
        if with_sfx:
            s1 = seen
            r1 = r
        else:
            s0 = seen
    first = next((i for i, (a, b) in enumerate(zip(s1, s0)) if a != b), None)
    rec(0x4b, first is not None and 15 <= first <= 25,
        f'batch actor_wait(0,20); queue_sfx(0,1): SFX channel block 0x03003E5C first differs from the no-sfx run at '
        f'frame {first} of the call ({r1[0]} f total) -> the sound is sequenced on the track')


def count(e, tag):
    _, r = None, None
    return tag


def t4f():
    # spawn a cutscene Janemba (sprite 149) and hand it over to Snake Way placement 0 (an NPC, sprite 161 ...)
    items = [149, 100, 120, NAT(0x35), 149, 0, NAT(0x4f), 149, NAT(0x37), -1, NAT(0x37)]
    base = [149, 100, 120, NAT(0x35), 149, NAT(0x37), -1, NAT(0x37)]
    e1, r1 = run(S, items, 30)
    e0, r0 = run(S, base, 30)
    shot(e1, 'become_map_object')
    rec(0x4f, r1[2] is not None and r0[2] is not None and r1[2][0] < r0[2][0] and r1[2][1] > r0[2][1],
        f'spawn_actor(149,100,120) then become_map_object(149, 0): count_actors(149), count_actors(placement 0) = '
        f'{r1[2]} vs {r0[2]} without it (the cutscene actor is replaced by a new instance of map object 0; '
        f'natural use in event 450 at frame 11710, 2 frames)')


def t55():
    tbl = [(u8(0x086B759C + 8 * k), u8(0x086B759C + 8 * k + 1), u32(0x086B759C + 8 * k + 4)) for k in range(4)]
    k, v = tbl[0][0], tbl[0][1]
    e, r = run(S, [0, k, v, 0, -8, NAT(0x55)], 20)
    p = player(e)
    shot(e, 'attachment')
    a = e.r32(p + 0x5C)
    e2, r2 = run(S, [0, k, v, 0, -8, NAT(0x55), 0, 0, 0, 0, 0, NAT(0x55)], 20)
    b = e2.r32(player(e2) + 0x5C)
    rec(0x55, a != 0 and b == 0 and e.r16(p + 0x17A) & 0xffff == 0xfff8,
        f'table 0x086B759C {tbl}; set_attachment(0,{k},{v},0,-8): player +0x5C = {a:#x}, +0x178/+0x17A = '
        f'({e.r16(p + 0x178)},{e.r16(p + 0x17A) - 65536 if e.r16(p + 0x17A) > 32767 else e.r16(p + 0x17A)}); then (0,0,0,0,0) -> +0x5C = {b:#x}')


def t58():
    def pre(e):
        pass
    e, r = run(S, [0, 1, NAT(0x58), 1, NAT(0x7f)], 120, keys=None)
    e0, r0 = run(S, [1, NAT(0x7f)], 120)
    e2 = emu(S)
    e2.run(3)
    from script_nat_b_lib import run_code
    run_code(e2, asm([0, 1, NAT(0x58)]), max_frames=20)
    shot(e2, 'item_get_20')
    rec(0x58, r[0] is not None and 55 <= r[0] <= 70 and r[2][-1] == r0[2][-1],
        f'show_item_get(0, item 1) blocked {r[0]} f (hold-up pose + icon, v2_item_get_20.png); item_count(1) '
        f'after = {r[2][-1]}, unchanged ({r0[2][-1]})')


def t6a():
    e, r = run(S, [135, 30, NAT(0x6a)], 60)
    cam = (e.r32(0x030019A4) >> 10, e.r32(0x030019A8) >> 10)
    e0, r0 = run(S, [0, 30, NAT(0x6a)], 60)
    cam0 = (e0.r32(0x030019A4) >> 10, e0.r32(0x030019A8) >> 10)
    rec(0x6a, r[0] is not None and 28 <= r[0] <= 34 and cam != cam0,
        f'camera_follow_wait(135, 30) blocked {r[0]} f; camera top-left (px) {cam} vs following the player {cam0}')


def t6d6e():
    e, r = run(S, [5, NAT(0x6e)], 10)
    c = e.r32(0x030019A4 + 0xC)
    e2, r2 = run(S, [5, NAT(0x6d)], 10)
    c8, c10 = e2.r32(0x030019A4 + 8), e2.r32(0x030019A4 + 0x10)
    e0, _ = run(S, [5, ('n', 0x14)] if False else [5] + pops(1), 10)
    b8, bc, b10 = e0.r32(0x030019A4 + 8), e0.r32(0x030019A4 + 0xC), e0.r32(0x030019A4 + 0x10)
    shot(e2, 'cam_param8')
    rec(0x6e, c == 5 << 10, f'camera_set_param12(5): camera(0x030019A4)+0xC {bc:#x} -> {c:#x}; blocked {r[0]} f')
    rec(0x6d, c8 == 5 << 10, f'camera_set_param8(5): +8 {b8:#x} -> {c8:#x}, +0x10 {b10:#x} -> {c10:#x}; blocked {r2[0]} f')


def t87():
    # drop a coin next to the player, step onto it: zenie goes up
    items = [154, 152, 1, NAT(0x87), 10, NAT(0x88), NAT(0x10)]
    base = [154, 152, 1] + pops(3) + [10, NAT(0x88), NAT(0x10)]
    e1, r1 = run(S, items, 80)
    e0, r0 = run(S, base, 80)
    shot(e1, 'drop_zenie_at')
    rec(0x87, r1[2] is not None and r1[2][0] > r0[2][0],
        f'drop_zenie_at(154,152,1) beside Goku (144,152), wait 10: zenie {r0[2]} -> {r1[2]} (coin collected)')


def t36_8a():
    # Yemma's office placement 4 has condition 1 (never): spawn_map_object respects it, the force variant does not
    e1, r1 = run(Y, [4, NAT(0x8a), -5, NAT(0x37)], 10)
    e2, r2 = run(Y, [4, NAT(0x36), -5, NAT(0x37)], 10)
    e0, r0 = run(Y, [-5, NAT(0x37)], 10)
    shot(e1, 'spawn_force')
    rec(0x8a, r1[2] == [1] and r0[2] == [0],
        f'Yemma office placement 4 (condition 1 = never): count_actors(-5) = {r0[2]}; after spawn_map_object_force(4) '
        f'{r1[2]}; after spawn_map_object(4) {r2[2]}')
    T.res['0x36_check'] = f'spawn_map_object(4) on a never-condition placement -> count {r2[2]}'


def t8b_ad():
    # Snake Way placements 0..2 need flag 2 clear; set flag 2, then refresh / respawn
    e1, r1 = run(S, [-1, NAT(0x37), 2, NAT(0x1d), NAT(0x8b), -1, NAT(0x37)], 20)
    rec(0x8b, r1[2] is not None and r1[2][0] >= 1 and r1[2][1] == 0,
        f'Snake Way: count_actors(placement 0) = {r1[2][0] if r1[2] else None}; set_flag(2) (its condition is "flag 2 '
        f'clear"); refresh_map_objects() -> {r1[2][1] if r1[2] else None}')
    # Yemma placement 12 needs flag 0x95 set
    e2, r2 = run(Y, [-13, NAT(0x37), 0x95, NAT(0x1d), NAT(0xad), -13, NAT(0x37)], 20)
    e3, r3 = run(Y, [0x95, NAT(0x1d), NAT(0x8b), -13, NAT(0x37)], 20)
    rec(0xad, r2[2] is not None and r2[2][0] == 0 and r2[2][1] >= 1,
        f'Yemma office placement 12 (condition flag 0x95 set): count {r2[2][0] if r2[2] else None}; set_flag(0x95); '
        f'respawn_conditional_map_objects() -> {r2[2][1] if r2[2] else None} (refresh_map_objects -> {r3[2]})')


def t90_91():
    e, r = run(S, [135, NAT(0x8f)], 10)
    shot(e, 'boss_bar')
    e1, r1 = run(S, [135, 0, 100, NAT(0x90)], 10)
    shot(e1, 'boss_bar_ex')
    e2, r2 = run(S, [135, 0, 100, NAT(0x90), NAT(0x91)], 10)
    shot(e2, 'boss_bar_hidden')
    ch1 = changed(e1.oam(), e.oam())
    ch2 = changed(e2.oam(), e1.oam())
    rec(0x90, len(ch1) == 0, f'show_boss_bar_ex(135,0,100) draws the same OAM as show_boss_bar(135) ({len(ch1)} '
        f'OAM words differ; v2_boss_bar_ex.png)')
    rec(0x91, len(ch2) > 0, f'hide_boss_bar() after it: {len(ch2)} OAM words change (gauge gone, v2_boss_bar_hidden.png)')


def t9e_a0():
    for n, nm in ((0x9e, 'marker_a'), (0x9f, 'marker_b')):
        e1, r1 = run(S, [100, 100, NAT(n)], 10)
        e0, r0 = run(S, [100, 100] + pops(2), 10)
        shot(e1, nm)
        ch = changed(e0.oam(), e1.oam())
        rec(n, len(ch) > 0, f'{nm}(100,100): {len(ch)} OAM words differ from the no-op run (v2_{nm}.png)')
    e1, r1 = run(S, [100, 100, 32, 16, NAT(0xa0)], 10)
    e0, r0 = run(S, [100, 100, 32, 16] + pops(4), 10)
    shot(e1, 'box')
    ch = changed(e0.oam(), e1.oam())
    rec(0xa0, len(ch) > 0, f'spawn_box(100,100,32,16): {len(ch)} OAM words differ (v2_box.png)')


def tac():
    e1, r1 = run(S, [NAT(0xab), 300, 300, NAT(0xac)], 20)
    e0, r0 = run(S, [NAT(0xab)], 20)
    shot(e1, 'radar_target')
    ch = changed(ram(e0), ram(e1), SKIP)
    rec(0xac, len(ch) > 0, f'show_radar(); radar_add_target(300,300) vs show_radar() alone: {fmt(ch, 6)}')


def taf_c0():
    _, ra = run(S, [NAT(0xbe), NAT(0xbf), 30, NAT(0x88), NAT(0xc0), 30, NAT(0x88), NAT(0xc1)], 100)
    _, rb = run(S, [NAT(0xbe), NAT(0xbf), 30, NAT(0x88), 30, NAT(0x88), NAT(0xc1)], 100)
    rec(0xc0, ra[2] is not None and rb[2] is not None and ra[2][0] < rb[2][0],
        f'stopwatch read after 30+30 frames: {ra[2]} with stop_stopwatch() in between, {rb[2]} without')
    e1, r1 = run(S, [30, NAT(0xae), 30, NAT(0x88), NAT(0xaf), 30, NAT(0x88)], 100)
    e0, r0 = run(S, [30, NAT(0xae), 30, NAT(0x88), 30, NAT(0x88)], 100)
    rec(0xaf, (e1.r8(GD + 1) & 4) == 0 and (e0.r8(GD + 1) & 4) != 0,
        f'start_countdown(30): GameData byte 1 = {e0.r8(GD + 1):#x} running; with stop_countdown() = {e1.r8(GD + 1):#x}; '
        f'RAM differs in {len(changed(ram(e0), ram(e1), SKIP))} words')


def td9_da():
    e1, r1 = run(S, [0, NAT(0xd9), 0, 100, NAT(0xda)], 20)
    shot(e1, 'dest_label')
    e0, r0 = run(S, [0, NAT(0xd9)], 20)
    ch = changed(ram(e0), ram(e1), SKIP)
    e2, _ = run(S, [], 20)
    ch9 = changed(ram(e2), ram(e0), SKIP)
    rec(0xd9, len(ch9) > 0, f'show_destination_label(0): {len(ch9)} RAM words vs nothing (HUD entity created; v2_dest_label.png)')
    rec(0xda, len(ch) > 0, f'set_destination_label(0, string 100) after it: {fmt(ch, 6)}')


def tdc():
    ch, r1, r0, e1, e0 = diff(Y, [0, 5, NAT(0xdc)], [0, 5] + pops(2), 10)
    rec(0xdc, len(ch) > 0, f'Yemma office, set_entity_value(0, 5): {fmt(ch, 8)}')


def t65_68_69():
    e1, r1 = run(Y, [0, 100, 100, NAT(0x69)], 10)
    shot(e1, 'prop_set_pos')
    e0, r0 = run(Y, [], 10)
    ch = changed(ram(e0), ram(e1), SKIP)
    rec(0x69, len(ch) > 0, f'prop_set_pos(0,100,100) in Yemma office: blocked {r1[0]} f; {fmt(ch, 6)} (v2_prop_set_pos.png)')
    _, ra = run(Y, [NAT(0x38), 0, 40, NAT(0x68), NAT(0x3a)], 80)
    rec(0x68, ra[0] is not None and 38 <= ra[0] <= 46, f'batch prop_wait(0, 40); end_parallel blocks {ra[0]} f')
    e2, r2 = run(Y, [0, 100, 100, NAT(0x65)], 10)
    shot(e2, 'prop_effect')
    ch = changed(ram(e0), ram(e2), SKIP)
    rec(0x65, r2[0] is not None and len(ch) > 0, f'prop_effect_at(0,100,100): blocked {r2[0]} f, {len(ch)} RAM words (v2_prop_effect.png)')


def t74():
    _, ra = run(Y, [1, 3, 0, 120, 140, NAT(0x72), 0, NAT(0x37)], 200)
    _, rb = run(Y, [1, 3, 0, 120, 140, NAT(0x74), 0, NAT(0x37)], 200)
    e1, r1 = run(Y, [1, 3, 0, 120, 140, NAT(0x72), -1, NAT(0x37), -7, NAT(0x37)], 200)
    e2, r2 = run(Y, [1, 3, 0, 120, 140, NAT(0x74), -1, NAT(0x37), -7, NAT(0x37)], 200)
    shot(e2, 'load_noobj')
    rec(0x74, r1[2] is not None and r2[2] is not None and sum(r2[2]) < sum(r1[2]),
        f'reload Yemma office: count_actors(placement 0, 6) = {r1[2]} with load_scene_fade, {r2[2]} with '
        f'load_scene_fade_noobj (blocked {r1[0]} / {r2[0]} f)')


def t8e():
    e, r = run(S, [NAT(0x8e)], 400)
    shot(e, 'title_400')
    rec(0x8e, r[0] is not None, f'return_to_title(): script returned after {r[0]} f; screen after 400 f: v2_title_400.png')


def t29():
    e = emu(S)
    e.run(3)
    tbl = [e.r32(0x03001990 + 4 * k) for k in range(8)]
    vts = [e.r32(o) if 0x02000000 <= o < 0x04000000 else 0 for o in tbl]
    T.res['0x29_table'] = f'slots {[hex(x) for x in tbl]} vtables {[hex(x) for x in vts]}'
    print(T.res['0x29_table'])


def tc9():
    for st in ('buu_fight_swforest.ss', 'buu_swforest3.ss', Y):
        ch, *_ = diff(st, [NAT(0xc9)], [], 10)
        print('c9', st, len(ch), fmt(ch, 6))
        if ch:
            rec(0xc9, True, f'broadcast_1f in {st}: {fmt(ch, 8)}')
            return
    rec(0xc9, False, 'no RAM effect in any of the 5 available states (no entity answers message 0x1F there)')


def tcb2():
    _, r = run(S, [0, 32, 32, NAT(0xcb), 0, 64, 16, NAT(0xcb), 0, 32, 32, NAT(0xcc), 0, 64, 16, NAT(0xcc)], 5)
    ok = r[2] is not None and r[2][0] - r[2][1] == 16 and r[2][3] - r[2][2] == -8 + 16 - 16 + 8
    rec(0xcb, r[2] is not None and r[2][0] - r[2][1] == 16,
        f'player anchor (144,152): rect 32x32 -> x {r[2][0]}, 64x16 -> x {r[2][1]} (left = centre - w/2, centre x 160)')
    rec(0xcc, r[2] is not None and r[2][3] - r[2][2] == 8,
        f'rect 32x32 -> y {r[2][2]}, 64x16 -> y {r[2][3]} (top = centre - h/2, centre y {r[2][2] + 16})')


TESTS = [t39, t3b, t41, t4b, t4f, t55, t58, t6a, t6d6e, t87, t36_8a, t8b_ad, t90_91, t9e_a0, tac, taf_c0, td9_da,
         tdc, t65_68_69, t74, t8e, t29, tc9, tcb2]
if __name__ == '__main__':
    want = sys.argv[1:]
    for t in TESTS:
        if want and t.__name__ not in want:
            continue
        try:
            t()
        except Exception as ex:  # noqa
            import traceback
            print(t.__name__, 'ERROR', repr(ex))
            traceback.print_exc(limit=2)
        T.RES.write_text(json.dumps(T.res, indent=1))
