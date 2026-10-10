"""Boss facts for the 'janemba' group, for the battle simulator (JSON-serialisable).

Status tags: VERIFIED = measured in the emulator (scripts named in 'verified'), DISASM = read from code,
INFERRED = plausible reading not exercised.  Frames are 60 fps; px are map pixels; speeds px/frame.
Stats come from the enemy table 0x08711274 (enemies.json).  Damage always uses the normal formula
(notes.md section 4) with the named attacker object's stats.

Strike-pattern letters: A = JanembaHandStrikeA (hand appears at player x+4, from part 0x34),
B = JanembaHandStrikeB (hand at player x-20, from part 0x35).  Read from the state trees in ROM."""

HAND_HIT = dict(attacker='Janemba Hand object (type 0x36): STR 22', attack_type='0x086B8098 melee sub 2 (can crit)',
                multiplier=1024, hits=1, hit_window_hand_frames=[16, 55],
                hand_position='player position at hand creation + (4,0) [A] or (-20,0) [B]',
                status='VERIFIED (hit 33 f after strike start; hand 0x36 created every 57 f)')

BOSSES = {
    0x33: dict(
        name='Janemba',
        stats=dict(hp=8880, exp=20500, STR=0, POW=0, END=70, resist_melee=64, resist_energy=128,
                   flinch=False, note='resist_melee 64 = takes half melee damage; STR 0: all its damage '
                                      'comes from the hand objects (type 0x36, STR 22)'),
        spawn=dict(area='HFIL world 18 map 14', pos=[128, 88], record='0x0808CB70',
                   condition='story flag 123 set and flag 125 not set (cond script 0x0808CB5C)',
                   per_spawn_script='script #450 (0x08793934): defeat cutscene - actor 149 (Janemba) '
                                    'becomes actor 154 (Super Janemba) [INFERRED from the script; '
                                    'killing Janemba in the emulator starts a white-out transition, VERIFIED]',
                   status='DISASM'),
        body_parts=dict(types={'0x34': 'screen-left part (arm A), offset (-12,-22)',
                               '0x35': 'screen-right part (arm B), offset (+44,-22)',
                               '0x37': 'upper part, offset (+16,-41), mirrors body anim'},
                        hp=0, status='VERIFIED (offsets measured)'),
        phases=[
            dict(state='0xFC', what='create body parts 0x34/0x35/0x37', length=4, status='VERIFIED'),
            dict(state='0xFD', what='idle until 240 f after spawn (IfCooldownOver 240)', length=239,
                 status='VERIFIED'),
            dict(state='0xFE', what='10 hand strikes back to back, 57 f each', pattern='BABAABABAA', length=573,
                 status='VERIFIED'),
            dict(state='0x101 / 0x102 / 0x103',
                 what='JanembaAscend (129 f, invulnerable) + JanembaDescendSummon (tops Mini-Janembas up to 5 '
                      'at fixed arena positions) - 230 f total; then HP check',
                 length=230, status='VERIFIED'),
            dict(state='0x104 / 0x105 / 0x106', what='HP >= 10%: wait (attackable, standing) until every '
                                                      'Mini-Janemba is dead', status='VERIFIED'),
            dict(state='0xFF', what='then 10 strikes AABABBBABA', pattern='AABABBBABA', length=571,
                 status='VERIFIED'),
            dict(state='0x100', what='(third cycle) 10 strikes AAABABABBB', pattern='AAABABABBB',
                 status='DISASM'),
            dict(state='0x108 / 0x109 / 0x107',
                 what='HP < 10% (IfHpBelow 0x1999): after the summon it does NOT wait for the minis; 10 '
                      'strikes each followed by Wait 24 (81 f per strike) - patterns 0x108 AABABBBABA, '
                      '0x109 AAABABABBB, 0x107 BABAABABAA',
                 length=812, status='VERIFIED (0x108 lasted 812 f; minis kept spawning up to 5)'),
        ],
        cycle='0xFE strikes -> 0x101 summon -> [wait minis | <10%: 0x108 strikes] -> 0xFF/0x108 strikes -> '
              '0x102 summon -> [wait | 0x109] -> 0x100/0x109 strikes -> 0x103 summon -> [wait | 0x107] -> '
              '0xFE ...',
        attacks=[dict(name='hand strike A/B', **HAND_HIT)],
        movement='none (stationary); rises off-screen at 3 px/frame during JanembaAscend and descends at '
                 '3 px/frame during the summon',
        invulnerability='no damage while airborne (JanembaAscend); VERIFIED (ComputeDamage called, HP unchanged)',
        summons=dict(type='0x38 Mini-Janemba', count='top-up to 5 each summon',
                     positions=[[104, 112], [120, 136], [152, 144], [184, 136], [200, 112]],
                     status='VERIFIED'),
        heals='none',
        verified=['bv_janemba_early.py 0x33 (timeline, part chain)', 'bv_janemba_early.py 0x33 2400 1100 '
                  '(kill minis -> leaves 0x104)', 'bv_janemba_early.py 0x33 2400 - 0.05 (<10% branch)',
                  'bv_janemba_vuln.py (airborne invulnerable, attackable while minis live)',
                  'bv_janemba_death.py 0x33 (death -> white-out transition)'],
    ),
    0x7A: dict(
        name='Super Janemba',
        stats=dict(hp=25840, exp=20500, STR=47, POW=70, END=55, resist_melee=128, resist_energy=128,
                   flinch=False),
        spawn=dict(area='HFIL world 18 map 14', records=['0x0808CB98', '0x0808CBC0'], condition='always (1); '
                   'placed by cutscene scripts #451 / #452 (records at (0,0))', status='DISASM/INFERRED'),
        phases=[
            dict(state='0x2D2', what='idle until player within 72 px (IfPlayerWithin2 r2=5184)', status='DISASM'),
            dict(state='0x2D3 (decision, every frame)', what=(
                'SubState 0x2DE (reactions) first; player > 120 px -> 0x2D4 chase 0.8984 px/f; '
                '50 < d <= 120: Chance 80%/f -> {15%/f: 0x2DB teleport+melee, 20%/f: 0x2DA wave}; '
                'else (d <= 50 or the rolls fail) 100%: {50%: 0x2D5 melee approach, 70% of rest: 0x2D7 step '
                '0.7969 px/f, else 0x2D8 step 1.0977 px/f}'), status='DISASM'),
            dict(state='0x2DE (reactions, checked inside 0x2D3/0x2D4/0x2D5/0x2D7)', what=(
                'HP < 25% -> 0x2DC berserk (permanent).  After 90 f in a state: player attacking me -> '
                '20%/f block (0x2D9); I am in the player beam line -> 80%/f block'), status='DISASM'),
            dict(state='0x2DC berserk (HP < 25%)', what='TeleportChain4 (4 teleports 72 f apart: below, '
                 'above, left, right of the player, 32 px) + FacePlayer + MeleeAttack, loop forever',
                 loop_frames=306, status='VERIFIED'),
        ],
        attacks=[
            dict(name='melee (0x2D5 approach -> 0x2D6)', node='MeleeAttack', attack_type='0x086B8098 melee sub 2',
                 multiplier=1024, attacker='STR 47', follow_up='Wait 6, then 50% chance to repeat if the player '
                 'is still in melee range', measured='~191-196 HP per hit on Goku L60 (END 26)',
                 status='VERIFIED'),
            dict(name='teleport + melee (0x2DB)', sequence='TeleportNearPlayer (frame 36 reappears 32 px from the '
                 'player, random side) -> FacePlayer -> MeleeAttack', state_length=90, status='VERIFIED'),
            dict(name='energy wave (0x2DA)', sequence='AlignWithPlayer(179/256 px/f) until aligned (0.8%/f '
                 'abort) -> SuperJanembaWave: spawn at frame 24, done at 40', projectile='super_janemba_wave',
                 status='VERIFIED'),
            dict(name='block (0x2D9)', sequence='ParallelAll(Block, until player stops attacking or 5%/f)',
                 status='DISASM'),
        ],
        movement=dict(chase_far=0.8984, chase_melee=1.1992, step_slow=0.7969, step_fast=1.0977, align=0.6992,
                      unit='px/frame (speed/256)', status='DISASM (node params)'),
        teleports='TeleportNearPlayer / TeleportChain4: 32 px from the player, frame 36 of each teleport',
        invulnerability='none special (normal enemy; does not flinch)',
        heals='none',
        verified=['bv_janemba_super.py (state log, teleports, melee damage)', 'bv_janemba_super.py 900 0.2 '
                  '(berserk loop)', 'bv_janemba_shot.py (wave speed, box, damage)'],
    ),
    0x38: dict(
        name='Mini-Janemba',
        stats=dict(hp=1475, exp=1250, STR=20, POW=0, END=56, resist_melee=128, resist_energy=128,
                   flinch=True, hit_reaction='standard (knockback + flinch, behaviour def 0x08708920)'),
        spawn=dict(how='summoned by Janemba (JanembaDescendSummon), 5 at fixed positions', status='VERIFIED'),
        phases=[
            dict(state='0x10F', what='HP < 10% -> 0x111 (flee hops) else 0x110 (hop toward)', status='VERIFIED'),
            dict(state='0x110', what='MiniHopToward; each frame 4% chance, if player within 47 px box and '
                                     'hittable -> 0x112', status='DISASM'),
            dict(state='0x112', what='MiniRollAttack (dash frames 16-31 at 1.5 px/f/axis, hit on frame 32) '
                                     '+ Wait 120', length=153, status='VERIFIED'),
        ],
        attacks=[dict(name='roll attack', attack_type='0x086B8088 melee sub 0', multiplier=1024,
                      attacker='STR 20', hit_frame=32, box='own body box', status='VERIFIED (timing)')],
        movement=dict(hop_cycle=32, airborne=16, speed_airborne=0.5, average=0.25, re_aim_chance=0.25,
                      status='VERIFIED'),
    ),
    0x39: dict(
        name='Janemba Hand (HFIL field enemy)',
        stats=dict(hp=50, exp=1125, STR=35, END=0, flinch=True),
        spawn=dict(area='HFIL world 18 maps 1-13', condition='inline 0x2007D', status='DISASM'),
        phases=[
            dict(state='0x114', what='SpawnPuff + Wait 120', length=122, status='VERIFIED'),
            dict(state='0x118', what='idle, Chance 1%/f -> 0x115 (mean 100 f)', status='VERIFIED'),
            dict(state='0x115', what='HandAmbush: teleport under the player (x+4 or x-20), hit frames 80-103',
                 length=106, status='VERIFIED'),
            dict(state='0x117', what='HandSink + Wait 180', length=287, status='VERIFIED'),
        ],
        attacks=[dict(name='ambush', attack_type='0x086B8098 melee sub 2', multiplier=1024, attacker='STR 35',
                      hit_window=[80, 103], status='VERIFIED')],
    ),
    0x36: dict(name='Janemba Hand (boss-spawned)', stats=dict(hp=50, exp=0, STR=22, flinch=False),
               notes='created by JanembaHandStrikeA/B under the player; Idle AI; its rising controller '
                     '0x0804C338 does the hit (see HAND_HIT); removed after its animation', status='VERIFIED'),
    0x34: dict(name='Janemba part (arm A)', notes='0 HP, Idle; animated by JanembaHandStrikeA', status='VERIFIED'),
    0x35: dict(name='Janemba part (arm B)', notes='0 HP, Idle; animated by JanembaHandStrikeB', status='VERIFIED'),
    0x37: dict(name='Janemba part (upper)', notes='0 HP; PartMirrorAnim', status='INFERRED'),
    0x8D: dict(name='floor trigger object (map 0/50, 12 placements)',
               notes='IfPlayerOnTop (16 px) -> TriggerSelf (own vfunc +0x74(1)); not a combatant; spawn '
                     'conditions are story-flag scripts', status='INFERRED'),
}

PROJECTILES = {
    'janemba_hand_36': dict(
        kind='enemy object (not a projectile class): Janemba Hand type 0x36 created under the player',
        ctor='0x0800F68C via spawn record 0x0877B050 (type 0x36, gfx 0x96)',
        speed_px_per_frame=0, lifetime_frames='until its rising animation ends (strike node ~57 f)',
        hitbox='hand collision box (0x0800FE78)', hit_window_frames=[16, 55], hits=1,
        damage=dict(attack_type='0x086B8098 (melee, sub 2, can crit)', multiplier=1024, stat='hand STR 22 '
                    'vs player END'), status='VERIFIED (timing), DISASM (damage fields)'),
    'super_janemba_wave': dict(
        ctor='builder 0x080359D4 (base ctor 0x08019B04, gfx arg 0x800)', vtable='0x0804B9B4',
        update='0x08035A81', speed_px_per_frame=2.625,
        velocity_table='0x08759688: down (0,672) up (0,-672) left (-672,0) right (672,0) /256',
        hitbox=dict(table='0x0875A6A8 + 12*dir (s8 ox, s8 oy, u8 w, u8 h)',
                    down_up='x-8 .. x+56, y-32 .. y (64 wide, 32 tall)',
                    left_right='x-32 .. x, y-8 .. y+56 (32 wide, 64 tall)',
                    measured_right=[-32, -8, 0, 56]),
        spawn='controller frame 24 of SuperJanembaWave; muzzle = boss bbox origin + (4,4)/(13,-24)/(-8,-8)/'
              '(24,-8) for down/up/left/right',
        lifetime_frames='no timer: until off-screen, wall (projectile passability map) or a hit',
        damage=dict(attack_type='0x086B8090 (energy, sub 1)', multiplier=1024, stat='owner POW 70 vs player '
                    'END2 / energy resist', measured='157 HP on Goku L60 (END2 26); model range 151-166'),
        status='VERIFIED (speed, box, spawn frame, damage), DISASM (tables)'),
}
