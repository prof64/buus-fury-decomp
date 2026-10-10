"""Extract player + enemy attacks -> data/log1/moves.json.

Player move projectiles are entities created by fire_move (0x0800B3DC) with state table
MOVE_STATE_TABLES[move] (5 callbacks per state: enter, update, anim-tick, visible, draw) and
sprite def MOVE_SPRITE_DEFS[move]. Numeric parameters come from the ROM tables listed per move
and from immediates in the callbacks (addresses given)."""
from romdata import *


def tbl(addr, n_rows, n_cols, keys):
    return {FACINGS[f]: dict(zip(keys, [s16(addr + 2 * (n_cols * f + k)) for k in range(n_cols)])) for f in range(n_rows)}


def state_table(addr, n):
    return [[h(u32(addr + 20 * i + 4 * j)) for j in range(5)] for i in range(n)]


def proj_size(defaddr):
    return dict(w=u16(defaddr), h=u16(defaddr + 2))


def main():
    names = [cstr(u32(MOVE_NAME_PTRS + 4 * i)) for i in range(3)]
    spawn = tbl(MOVE_SPAWN_OFFSETS, 4, 3, ['dx', 'dy', 'entity_flags_0C'])
    fly = tbl(KIBLAST_FLY_TABLE, 4, 4, ['dx', 'dy', 'vx', 'vy'])
    grow = tbl(KAME_GROW_TABLE, 4, 4, ['dx', 'dy', 'dw', 'dh'])
    retract_end = tbl(KAME_GROW_TABLE + 0x20, 4, 4, ['dx', 'dy', 'dw', 'dh'])
    retract_wall = tbl(KAME_GROW_TABLE + 0x40, 4, 4, ['dx', 'dy', 'dw', 'dh'])
    start_off = tbl(KAME_GROW_TABLE + 0x60, 4, 2, ['dx', 'dy'])
    charge = dict(
        description='While B is held and the owner is in its Ki state (5), every frame with frame_counter & 7 == 0 '
                    'the projectile takes `cost` ki from the owner (only if owner ki >= cost) and adds it to its '
                    'charge (+0x24). Function 0x0800B436.',
        cost_player='(maxKi + 15) >> 4  per 8 frames  (0x0800B50A)',
        cost_enemy='(power + 7) >> 3 per 8 frames until charge >= power (0x0800BF30)')
    moves = []
    moves.append(dict(
        id='melee', name='Punch (Melee1/Melee2)', user='player', input='A (keys-new bit 0) in Standing/Walking state',
        code=dict(input='0x08002E80', hit_test='0x08005604 (enemy msg 0)', damage='0x08005676'),
        ki_cost=0, damage='(player_maxHP * (0x180 + rand2(0x80))) >> 9   i.e. 75.0%..99.8% of max HP',
        rng='secondary RNG 0x0800A714 (state 0x03003D24); rand2(n) in [0, n-1]',
        hit_test='single point in front of the player must be inside the enemy hurtbox (sprite def, per facing); '
                 'only the first entity (scanning the entity list from the end) is hit',
        hit_point_offsets_from_player_pos=dict(down=[8, 8], up=[8, -8], left=[-2, -8], right=[18, -8]),
        hit_point_code='0x08003446',
        animation='state 3 (Melee1) or 4 (Melee2) chosen by frame_counter & 1; 16 frames (anim-tick 0x08002A52), '
                  'sfx 2; cannot act until it ends',
        startup_frames=0, active_frames=1, recovery_frames=16,
        effects='target: hurt state 6 -> stun 20 frames (char+0x0A) + invulnerable 80 frames (char+0x14), no knockback',
        verified='VERIFIED: 10/10 predicted damages (verify_melee.py); hit timing observed in emulator'))
    moves.append(dict(
        id=0, name=names[0], user='player', input='B (move slot 0, always available)',
        code=dict(fire='0x0800B3DC', states=h(u32(MOVE_STATE_TABLES)), charge='0x0800B4FA', fly_enter='0x0800B554',
                  fly_update='0x0800B5A2'),
        state_table=state_table(u32(MOVE_STATE_TABLES), 3),
        sprite_defs=dict(large=h(0x086B3AA8), medium=h(0x086B3AD4), small=h(0x086B3C50)),
        size_px=dict(large=proj_size(0x086B3AA8), medium=proj_size(0x086B3AD4), small=proj_size(0x086B3C50)),
        size_rule='charge value c = 10 + ki_spent: c > 72 -> large 16x16 (pos +0), c > 20 -> medium 8x8 (pos +4,+4), else small 6x6 (pos +5,+5)',
        ki_cost='0 for a tap; charging drains (maxKi+15)>>4 every 8 frames while held',
        charge=charge,
        damage='owner_maxHP // 3 (0x0800B596, independent of charge)',
        speed_px_per_frame=3, lifetime_frames='10 + ki_spent (counts down by 1 per frame, 0x0800B5BA)',
        range_px='3 * (10 + ki_spent) unless it hits a wall (collision layer 2) or an entity',
        hits='first entity whose handler accepts msg 1 (0x0800283C); projectile then enters state 2 (vanish 24 anim ticks)',
        spawn_offset=spawn, fly_offset_and_velocity=fly,
        also='destroys breakable obstacles (0x0800689A msg 1) and sends msg 6 to all entities',
        verified='VERIFIED: damage 200 at maxHP 600 for tap and 60-frame charge (test_ki.py); flight 10 frames for a tap'))
    moves.append(dict(
        id=1, name=names[1], user='player', input='B with slot 1 selected (L cycles slots, 0x08003524); must be learned (bit 1 of 0x0300004D)',
        learned='script command 0x08006216 (class 0x08166B28) instance at 0x08543DE4: taught by King Kai after the Gregory mallet training (map 13; text 0x081612E9)',
        code=dict(fire='0x0800B3DC', states=h(u32(MOVE_STATE_TABLES + 4)), release='0x0800B716', grow='0x0800BCD8',
                  draw='0x0800B778'),
        state_table=state_table(u32(MOVE_STATE_TABLES + 4), 3),
        sprite_def=h(0x086B376C),
        ki_cost='charge only: (maxKi+15)>>4 per 8 frames held',
        charge=charge,
        damage='ki_spent (beam charge +0x24) per hit; 0 if released without charging',
        sfx='0x0C if 2*charge >= maxKi else 0x08',
        beam='starts 8x8 at owner pos + spawn_offset + start_offset; each frame grows 3 px along facing (grow table); '
             'stops after 16 frames (max 56 px long) or when the head enters a wall; then 8-frame retract/fly-off',
        hits='every overlapping entity every growth frame (0x0800286A broadcast); msg flag +0x10=1 makes enemies with '
             'invulnerability timer (char+0x14) > 0 ignore it -> each enemy is damaged once per 80 frames',
        spawn_offset=spawn, start_offset=start_off, grow_per_frame=grow, retract_end=retract_end, retract_wall=retract_wall,
        verified='VERIFIED: damage 175 = 7 charge ticks x 25 at maxKi 400 (test_ki.py)'))
    moves.append(dict(
        id=2, name=names[2], user='player', input='B with slot 2 selected; must be learned (bit 2 of 0x0300004D)',
        learned='script command instance at 0x08544070, just before the script warps to map 40 (Frieza area); text 0x08166468 "I shall teach you the technique of Solar Flare!"',
        code=dict(fire='0x0800B3DC', states=h(u32(MOVE_STATE_TABLES + 8)), release='0x0800B692', freeze='0x080087A0'),
        state_table=state_table(u32(MOVE_STATE_TABLES + 8), 2),
        sprite_def=h(0x086B3FFC),
        ki_cost='charge only; charge capped at maxKi',
        charge=charge,
        damage=0,
        effect='freezes ALL enemy AI for duration = 60 + charge*300 // maxKi frames (60..360). '
               'Global timer 0x03003B34 checked at the top of the AI tick 0x08008018; also sets every non-flying '
               'awake enemy to idle state and reduces stun timers.',
        cooldown='cannot fire again until frame_counter >= release_frame + duration + 60 (0x030007E4, checked in 0x08002F24)',
        sfx=0x0A, verified='INFERRED (static reading of 0x0800B692 / 0x080087A0)'))
    moves.append(dict(
        id='fly', name='Flight (R)', user='player', input='R toggles; requires a passable flying-layer point in front and flight energy > 0',
        code=dict(takeoff='0x08002EBE', update='0x08002A88', energy='0x08003290'),
        speed_px_per_frame=2, energy_cost='1 per 8 frames', max_energy='level table column 3 (6..99)',
        refill='pickup items 31/32/33 (+5/+10/+20, capped) via 0x080032A2; no natural regeneration',
        landing='when energy runs out or R pressed: lands if the ground below is passable, else returns to the takeoff point',
        verified='VERIFIED: 2 px/frame and energy 6 -> 0 in 48 frames at level 1; item 31 refilled 1 -> 6'))
    moves.append(dict(
        id='enemy_melee', name='Enemy melee', user='enemy', code='0x0800F774 (r2 <= 1 branch)',
        trigger='Manhattan tile distance to target <= 1 (tiles = 8 px, 0x0800F570)',
        damage='cur_hp // 4 + rand(cur_hp // 2)  (attacker CURRENT hp; main RNG, rand(n) in [0,n]; '
               'for cur_hp//2 >= 32768 rand can go NEGATIVE -> healing bug)',
        cap_vs_player='min(dmg, player_maxHP // 3 + 1) (0x03000C1C, 0x080074A0)',
        animation='attacker set to char state 5 (anim Melee1, 16 frames) then 45-frame cooldown (char+0x0A = 0x2D)',
        observed_interval_frames='69-77 for a wolf hugging the player',
        verified='VERIFIED: wolf (350 HP) hits 105..251 observed, formula range 87..262; negative-damage bug observed (henchman 70000 HP dealt -13557)'))
    moves.append(dict(
        id='enemy_ki_blast', name='Enemy ki blast', user='enemy', code='0x0800F808..0x0800F912, spawn 0x0800C010',
        trigger='Manhattan distance > 1, ki > maxKi // 16, target roughly in line (attacker point within target x or y '
                'band widened by half its size), rand(100) <= 1 (2/101 per decision), not in a cutscene (0x030007DC == 0)',
        power='rand(ki // 2) + ki // 4 ; projectile charges (spending owner ki) until charge >= power',
        damage='owner_maxHP // 3 (fly-enter 0x0800B554 overwrites the power)', cap_vs_player='player_maxHP // 3 + 1',
        size_px=proj_size(0x086B3AA8), sprite_def=h(0x086B3AA8), sprite_def_vegeta=h(0x086B3400),
        sprite_rule='owner name starting with "v" (vegeta) uses 0x086B3400',
        speed_px_per_frame=3, lifetime='until it leaves the camera rectangle or hits a wall/entity (0x0800BF4E)',
        state_table=state_table(u32(ENEMY_PROJ_STATE_TABLES), 3),
        verified='VERIFIED: damage 26666 (maxHP 80000) and 5000 (maxHP 15000) observed; ~58 frames charge'))
    dump('moves.json', dict(
        source='LoG1 (USA); re/log1/extract_moves.py. Facing order in all tables: down, up, left, right.',
        units=dict(px='pixels', frames='60 Hz frames', damage='hit points before defense (apply_damage 0x08007352)'),
        move_names_table=h(MOVE_NAME_PTRS), moves=moves))


if __name__ == '__main__':
    main()
