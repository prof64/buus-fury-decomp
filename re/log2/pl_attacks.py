"""Per-attack simulator records for moves.json (imported by extract_moves.py).

Static values come from ROM tables / code constants (address cited, tag DISASM); measured values come from
re/log2/pl_measurements.json (produced by pl_measure_all.py + pl_measure_compact.py on save states
work/states/log2_pl_*.ss: Lv40 characters on map 12 vs class-53 snakes; tag VERIFIED).

Conventions for every record:
  * coordinates are pixels relative to the player's integer position (entity+0x158>>8, +0x15C>>8); +x right, +y down.
  * rects are [x0, y0, x1, y1]; a hit happens when rect overlaps the target hurtbox (strict <).
  * facings: down, up, left, right (entity+0x10 = 0,1,2,3).
  * frames: frame 0 = the frame the button is pressed (state entered the same frame).
  * 'player_vulnerable': whether enemy attacks can damage the player during the move. The player is only
    protected when its damage multipliers (+0x163/+0x164) are 0 (hurt/revert states) or entity+0x168 == 0
    (special-melee release; checked in the player message handler at 0x08008A5C).
"""
import json, os
from common import ROM, u8, u32, s32, ROOT

FAC = ['down', 'up', 'left', 'right']
MEAS = json.load(open(os.path.join(ROOT, 're/log2/pl_measurements.json')))

def table(addr, stride, fields, n=4):
    """read per-facing table of s32 fields"""
    return {FAC[f]: [s32(addr + stride * f + 4 * k) for k in range(fields)] for f in range(n)}

def run_hits(run, attack_id=None):
    return [h for h in MEAS[run]['hits'] if attack_id is None or h['attack_id'] == attack_id]

def first_hit_rect(run, attack_id=None):
    h = run_hits(run, attack_id)
    return h[0]['rect_rel'] if h else None

def proj(run, i=0):
    p = MEAS[run]['projectiles']
    return p[i] if len(p) > i else None

def per_facing_hit_rects(runs, attack_id):
    out = {}
    for fac, run in runs.items():
        r = first_hit_rect(run, attack_id)
        if r: out[fac] = r
    return out

def state_len(run, state):
    st = MEAS[run]['player_states']
    for i, (f, s) in enumerate(st):
        if s == state:
            return (st[i + 1][0] - f) if i + 1 < len(st) else None
    return None

def knockback(run):
    e = MEAS[run]['enemies'][0]
    hurt = [c for c in e['state_changes'] if c[1] == '0x3002128']
    return dict(enemy_total_push_px=e['moved_px'], enemy_entered_generic_hurt_state_frame=hurt[0][0] if hurt else None,
                note='measured on a free snake (class 53); push direction = away from the player; the push itself is the '
                     'enemy-side hurt reaction (generic actor state 0x02 @0x03002128, ~31 frames) - see Part C')

MELEE_RECTS = table(0x083B5144, 16, 4)          # basic punch / energy punch (also 0x086DB28C)
SPMELEE_RECTS = table(0x083B5208, 16, 4)        # charged special melee
SWORD_SWING = per_facing_hit_rects({'right': 'swordblast_r'}, 12)

def build():
    A = []
    # ---------------- basic punch
    A.append(dict(
        name='Punch (A)', characters='all (Hercule weaker by stats only)', input='tap A', player_state='0x28/0x29/0x2A (random)',
        attack_table_id=2, power=0x400, type='melee', scale=1024, ep=None,
        timing=dict(startup_frames=0, active_frames=1, total_state_frames=16, recovery_frames=16,
                    note='hit resolves on the press frame (0x08007DE8); next A accepted after the 16-frame anim (VERIFIED)'),
        hitbox_rect_by_facing=MELEE_RECTS, hitbox_measured=per_facing_hit_rects(
            {'down': 'punch_d', 'up': 'punch_u', 'left': 'punch_l', 'right': 'punch_r_free'}, 2),
        hits=dict(count=1, targets='every enemy overlapping the rect'), projectile=None,
        player_vulnerable=True, stun_sent=None, knockback_given=knockback('punch_r_free'),
        evidence='VERIFIED: punch_r_free, punch_d/u/l'))
    # ---------------- charged special melee
    A.append(dict(
        name='Charged special melee (Super Kick / Spin Punch(Whirlspin) / Two-Handed Smash / Cross Slash / Flurry Punch)',
        characters='Gohan, Piccolo, Vegeta, Trunks, Goku (needs flag bit2)', input='punch, keep A held, release when charged',
        player_state='0x27', attack_table_id=4, power=0x600, type='melee', scale=1024, ep=None,
        timing=dict(charge='after the 16-frame punch: form.charge_anim_quads*4 frames + 16-frame countdown (0x0800839A); releasing earlier cancels',
                    min_hold_frames_from_press='16 + 4*charge_anim_quads + 16 (=32 for most forms; VERIFIED cancel at 30, works at 110)',
                    release='hits on attack-anim frames k*4 for bits k of form.hit_quads_mask, counted from the release; ends after form.anim_len_quads*4 frames',
                    measured={'Goku/Gohan/Vegeta (mask 0x18, len 6)': 'release f112 -> hits f119-123 and f127, state ends f135 (23 f)',
                              'Trunks (mask 0x60, len 7)': 'hits f123,f127, ends f131',
                              'Piccolo (mask 0x1E, no lunge)': '4 hits f115,f119,f123,f127, ends f135'}),
        lunge='form+0x0C px/f (1.0 for most, 0 for Piccolo/Hercule) along facing during the release (VERIFIED: player x +16 px over the swing)',
        hitbox_rect_by_facing=SPMELEE_RECTS,
        hitbox_measured={'right': first_hit_rect('spmelee_goku', 4), 'down': first_hit_rect('spmelee_goku_d', 4)},
        hits=dict(count='popcount(form.hit_quads_mask): 2 (most), 4 (Piccolo)', rehit_interval_frames=4, targets='all overlapping'),
        per_form='characters.json forms[].special_melee',
        player_vulnerable='charging: yes; release: no (entity+0x168=0, 0x080083FA..0x08008565; DISASM + measured flag)',
        projectile=None, stun_sent=None, evidence='VERIFIED: spmelee_goku/_gohan/_vegeta/_trunks/_piccolo_imm/_goku_d/_goku_short'))
    # ---------------- energy blast
    p = proj('blast_d')
    A.append(dict(
        name='Energy blast (basic ki blast)', characters='all except Hercule (slot 3)', input='B (tap again within the window to rapid-fire)',
        player_state='0x22', attack_table_id=3, power=0x400, type='energy', scale=1024,
        ep=dict(start=2, min=2, per_extra_shot=2),
        timing=dict(startup_frames=8, refire_period_frames=16, total_state_frames=24, recovery_frames_after_last_shot=16,
                    rapid_fire_measured='shots at f10,26,42,58 for taps every 6 f (blast_rapid)'),
        projectile=dict(vtable='0x08024CBC', speed_px_per_frame=3.0, size=[16, 16],
                        spawn_pos_by_facing={'down': proj('blast_d')['spawn_pos_rel'], 'up': proj('blast_u')['spawn_pos_rel'],
                                             'left': proj('blast_l')['spawn_pos_rel'], 'right': proj('blast_free')['spawn_pos_rel']},
                        rect_by_facing_at_spawn={'down': proj('blast_d')['spawn_rect_rel'], 'up': proj('blast_u')['spawn_rect_rel'],
                                                 'left': proj('blast_l')['spawn_rect_rel'], 'right': proj('blast_free')['spawn_rect_rel']},
                        lifetime='until the rect leaves the camera view (0x08013C8E) - 40 frames / ~117 px to the screen edge in the test map',
                        on_hit='stops at the first target and plays a 15-frame impact (no further damage)'),
        hits=dict(count=1, pierce=False), player_vulnerable=True, stun_sent=None, knockback_given=knockback('blast_free'),
        evidence='VERIFIED: blast_r_pierce (2nd enemy not hit), blast_d/u/l, blast_free, blast_rapid'))
    # ---------------- Masenko
    A.append(dict(
        name='Masenko Ball', characters='Gohan (slot 0)', input='hold B to charge range, release to throw', player_state='0x2D',
        attack_table_id=7, power=0x800, type='energy', scale='min(2*C+256, 2048), C=clamp(-180+30*frames_held, 255, 1024)',
        ep=dict(start=4, min=4, drain=None),
        timing=dict(ball_spawns_frame=0, launch='on B release', flight_frames_after_release=36,
                    state_ends_frames_after_release=32, measured={'tap': 'explodes f38 (scale 766)', 'hold30': 'explodes f67 (scale 1696)'}),
        projectile=dict(vtable='0x08024CE8', ctor='0x080139BC', spawn_offset_table_rom='0x086E374C (dx,dy,anim per facing)',
                        spawn_offset_by_facing=table(0x086E374C, 12, 3), spawn_pos_measured={'right': proj('masenko_tap')['spawn_pos_rel'], 'down': proj('masenko_d')['spawn_pos_rel']},
                        size='8x8 in flight', velocity='horizontal C/256 px/f along facing; vertical arc vy0=-2 px/f, +0.125 px/f per frame',
                        range_px='~36*C/256 (34 px tap, 136 px max)'),
        explosion=dict(rect_size=[16, 16], rect_measured_right=first_hit_rect('masenko_tap', 7), duration='single hit frame', targets='all overlapping'),
        hits=dict(count=1, pierce='n/a (explodes at the end of the arc, passes over enemies before that)'),
        player_vulnerable=True, stun_sent=None, evidence='VERIFIED: masenko_tap (3 enemies hit by one 16x16 blast), masenko_hold30, masenko_d'))
    # ---------------- Kamehameha
    A.append(dict(
        name='Kamehameha', characters='Goku (slot 0), Gohan (slot 1)', input='hold B (charge 24 f, then beam while held)', player_state='0x30',
        attack_table_id=10, power=0x40, type='energy', scale=1024,
        ep=dict(start=2, min=12, drain='86/256 EP per frame while the beam is out (~1 EP / 3 f), 0x080152A6'),
        timing=dict(charge_frames=24, cancel='B released before frame 24 -> no beam', beam_until='B released or EP 0',
                    first_hit_frame_measured=26, recovery_frames=0),
        beam=dict(object_ctor='0x080143A8', head_box=[16, 16], grow_px_per_frame=3,
                  head_rect_measured={'right': first_hit_rect('kame_r_2', 10), 'down': first_hit_rect('kame_d', 10)},
                  behaviour='head advances 3 px/f until it overlaps an enemy, a wall or the screen edge, then stays; only the head box deals damage'),
        hits=dict(rehit_interval_frames=1, pierce=False, note='hits the first enemy every frame (2nd enemy behind never hit); sometimes 2 hits/frame when two beam segments overlap'),
        player_vulnerable=True, stun_sent='continuous: target is re-hit every frame so it stays in its hurt state', knockback_given=knockback('kame_free'),
        evidence='VERIFIED: kame_r_2, kame_d, kame_free'))
    # ---------------- SBC
    A.append(dict(
        name='Special Beam Cannon', characters='Piccolo (slot 0)', input='hold B', player_state='0x31',
        attack_table_id=11, power=0x66, type='energy', scale=1024,
        ep=dict(start=2, min=8, drain='34/256 EP per frame (~1 EP / 7.5 f), 0x080153F8'),
        timing=dict(charge_frames=24, first_hit_frame_measured=25, beam_until='B released or EP 0'),
        beam=dict(object_ctor='0x080157E0', thickness_px=12, grow_px_per_frame=3,
                  rect_measured={'right_first': first_hit_rect('sbc_r_2', 11), 'right_last': run_hits('sbc_r_2', 11)[-1]['rect_rel'],
                                 'down_first': first_hit_rect('sbc_d', 11)},
                  behaviour='one long rect from the muzzle to the tip; tip grows 3 px/f and is not stopped by enemies'),
        hits=dict(rehit_interval_frames=1, pierce=True, note='every enemy inside the beam is hit every frame (VERIFIED: 2nd enemy 30 px behind hit from f39)'),
        player_vulnerable=True, stun_sent='continuous re-hit', evidence='VERIFIED: sbc_r_2, sbc_d'))
    # ---------------- Scatter Shot
    A.append(dict(
        name='Scatter Shot', characters='Piccolo (slot 1)', input='tap B', player_state='0x2C',
        attack_table_id=6, power=0x500, type='energy', scale=1024, ep=dict(start=12, min=12),
        timing=dict(startup_frames=16, total_state_frames=48, recovery_frames=32),
        projectile=dict(vtable='0x08024D14', count=3, size=[16, 16], table_rom='0x086E362C (per facing 3 x [dx,dy,vx,vy,param,anim], 24.8 velocities)',
                        pellets_by_facing={FAC[f]: [[s32(0x086E362C + 72 * f + 24 * i + 4 * k) for k in range(6)] for i in range(3)] for f in range(4)},
                        measured_right=[dict(spawn=x['spawn_pos_rel'], step=x['steady_step']) for x in MEAS['scatter_r']['projectiles']],
                        speed='centre 3.0 px/f; sides (2.598, +-1.5) px/f (30 deg spread)', lifetime='until off-screen or hit'),
        hits=dict(count='up to 3 (one per pellet)', pierce=False), player_vulnerable=True, stun_sent=None,
        evidence='VERIFIED: scatter_r, scatter_d'))
    # ---------------- Big Bang
    A.append(dict(
        name='Big Bang', characters='Vegeta (slot 0)', input='hold B to charge, release to fire', player_state='0x2E',
        attack_table_id=8, power=0xC000, type='energy', scale='32 + frames held (stops growing at EP 0)',
        ep=dict(start=10, min=20, drain='1 EP every 4 frames while held'),
        timing=dict(launch='on release', recovery_frames_after_release=32, measured={'tap': 'explodes f4 on an enemy 40 px away (scale 33)', 'hold40': 'explodes f56 (scale 72)'}),
        projectile=dict(vtable='0x080241B0', ctor='0x08013E48', spawn_offset_by_facing=table(0x086E377C, 12, 3),
                        velocity_by_facing_24p8=table(0x086DE904, 8, 2), speed_px_per_frame=2.0, initial_jump_px=16,
                        size='grows while charging: 8x8 -> 16x16 -> 32x32 (sprite tiers)', probe_box='24x24 around the ball each frame (msg type 2)',
                        lifetime='until it touches an enemy/wall or leaves the screen'),
        explosion=dict(rect_size=[64, 64], rect='ball position +-32', rect_measured=first_hit_rect('bigbang_tap', 8),
                       duration_frames=16, targets='all enemies in the box, once'),
        hits=dict(count=1, pierce=False), player_vulnerable=True, stun_sent=None,
        evidence='VERIFIED: bigbang_tap (3 enemies in blast), bigbang_hold40 (enemy 32+ px outside box not hit), bigbang_d'))
    # ---------------- Energy Punch
    A.append(dict(
        name='Energy Punch', characters='Vegeta (slot 1)', input='tap B', player_state='0x2B',
        attack_table_id=5, power=0x600, type='melee', scale=1024, ep=dict(start=4, min=4),
        timing=dict(startup_frames=0, total_state_frames=16), hitbox_rect_by_facing=MELEE_RECTS,
        hitbox_measured={'right': first_hit_rect('epunch_r', 5)}, hits=dict(count=1), player_vulnerable=True,
        evidence='VERIFIED: epunch_r'))
    # ---------------- Burning Attack
    A.append(dict(
        name='Burning Attack', characters='Trunks (slot 0)', input='B (hold to keep the bolt charging in front, release to fire)', player_state='0x2F',
        attack_table_id=9, power=0x40, type='energy', scale=1024, ep=dict(start=6, min=6),
        timing=dict(launch='on B release', recovery_frames_after_release=32),
        projectile=dict(vtable='0x0802507C', ctor='0x080141B4', spawn_offset_by_facing=table(0x086E37AC, 12, 3),
                        velocity_by_facing_24p8=table(0x086E15CC, 8, 2), speed_px_per_frame=2.5, initial_jump_px=10,
                        size='8x8 while held, 32x32 when flying', hit_rect_measured={'right': first_hit_rect('burning_tap', 9), 'down': first_hit_rect('burning_d', 9)},
                        lifetime='until a target/wall/off-screen; after a hit it fades 16 frames'),
        hits=dict(count=1, pierce=False, note='2nd enemy behind not hit'), player_vulnerable=True,
        stun_sent='attack id 9 (tiny damage); on the test snake it caused the normal 31-frame hurt state - any special stun is enemy-AI specific (Part C)',
        knockback_given=knockback('burning_free'), evidence='VERIFIED: burning_tap, burning_hold30, burning_free, burning_d'))
    # ---------------- Sword Blast
    A.append(dict(
        name='Sword Blast', characters='Trunks (slot 1)', input='tap B', player_state='0x32',
        attack_table_ids=[12, 13], power={'swing (id 12, melee)': 0x400, 'wave (id 13, energy)': 0xC00}, scale=1024, ep=dict(start=5, min=5),
        timing=dict(swing_hit_frame=0, wave_spawn_frame=8, total_state_frames=33),
        swing=dict(rect_measured=SWORD_SWING),
        projectile=dict(vtable='0x08024B24', speed_px_per_frame=3.5, size='8x16 (horizontal) / 16x8 (vertical)',
                        spawn_pos_measured={'right': proj('swordblast_r')['spawn_pos_rel'], 'down': proj('swordblast_d')['spawn_pos_rel']},
                        lifetime='until hit or off-screen (33 f to the screen edge in the far test)'),
        hits=dict(count='swing 1 + wave 1', pierce=False, note='wave stopped at the first enemy (enemies at 60/90 px not hit)'),
        player_vulnerable=True, evidence='VERIFIED: swordblast_r, swordblast_d'))
    # ---------------- Spirit Bomb
    A.append(dict(
        name='Spirit Bomb', characters='Goku (slot 1)', input='hold B to grow, release to throw', player_state='0x33',
        attack_table_id=14, power=0xA000, type='energy', scale='size = 5 + frames from press until the throw (throw = 8 f after release)',
        ep=dict(start=10, min=20, drain='1 EP every 4 frames while held (growth stops at EP 0)'),
        timing=dict(throw_frames_after_release=8, state_ends='~24 f after release (measured: release f42 -> state end f66)'),
        projectile=dict(vtable='0x08024AF8', ctor='0x080165EC', spawn_offset=[8, -24],
                        throw_velocity_by_facing_24p8=table(0x086F78F0, 8, 2), speed_px_per_frame=1.0,
                        size='8x8 -> 16x16 (size>=64) -> 32x32 (sprite tier min(4,size>>6))'),
        explosion=dict(rect='bomb rect expanded by its own width/height on every side (3x the bomb box): 24x24 small, 48x48 at size>=64',
                       rect_measured={'size23': first_hit_rect('spirit_hold10', 14), 'size53': first_hit_rect('spirit_hold40', 14), 'size143': first_hit_rect('spirit_hold130', 14)},
                       targets='all in the box'),
        hits=dict(count=1, pierce=False), player_vulnerable=True,
        stun_sent='if size >= 128 (0x08016726): attack id 15 (power 0) to the whole map -> enemies enter stun state @0x03002230 for 324 frames (VERIFIED on snakes); msg 0x13 broadcast at the throw',
        evidence='VERIFIED: spirit_hold10/40/130, spirit_free, spirit_stun_long'))
    # ---------------- Hercule special
    A.append(dict(
        name='Hercule special', characters='Hercule (all slots)', input='tap B', player_state='0x05',
        attack_table_id=15, power=0, type='energy', scale=0, ep=dict(start=6, min=6),
        timing=dict(total_state_frames=32),
        effect='sends attack id 15 with rect (0,0)-(2048,2048) to every entity: no damage, stun (state 0x03002230, 324 f on snakes)',
        player_vulnerable=True, evidence='VERIFIED: hercule_sp (rect/ids); stun duration from spirit_stun_long'))
    # ---------------- transformations
    A.append(dict(
        name='Transform / revert (move types 11, 12)', input='B with slot 2 selected', player_state='0x23 / 0x25 / 0x24',
        ep=dict(start=10, min=10), timing=dict(transform_frames='~165 (0x23) / 64 (0x25 Piccolo)', revert_frames='40-80'),
        player_vulnerable='during transform: yes; during revert 0x24: no (mults 0)', evidence='VERIFIED: pl_transform.py runs'))
    return A
