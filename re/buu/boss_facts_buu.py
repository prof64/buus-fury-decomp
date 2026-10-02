"""Boss facts for the Buu group: Majin Buu (0x1A/0x1B/0x1C), Super Buu (0x73-0x79), Kid Buu (0x3C/0x3D),
Babidi (0x04/0x19/0x1D).  JSON-serialisable.  Every numeric claim carries a tag:
  VERIFIED = measured in the emulator (scripts bv_buu_*.py, force-spawned on the SW-forest map),
  DISASM   = read from the code / ROM tables, INFERRED = interpretation.
Units: frames (60/s), px, px/frame; 'chance x%/f' = per-frame probability while that tree node runs.
Damage of every hit = formulas.json damage with the attack type + multiplier listed (attacker = boss obj,
stats from enemies.json: STR for melee kinds, POW for energy).
Selector chance lists 'Selector[Chance a -> X, Chance b -> Y, Z]' give P(X)=a, P(Y)=(1-a)b, P(Z)=rest.

Generic mechanisms found here (apply to every enemy, not only bosses):
  * state table entry = {behaviour root, on-hit 'aux' tree}; enemy table +0x14 = default aux.  When the
    enemy is hit the game runs word +4 (f1, onHit) of the aux def; composites forward to children.
    State 0's aux (0x086FDBC8) is the common hit reaction: hit attack byte 0x10 (Volleyball) -> push state
    0x10; 0x0A (Burning Attack) -> push 1 then 4; 0x0C (Flame Kamehameha) -> push 3; otherwise push 2
    (DISASM).  Handler 0x08029A41 = 'if hit attack-type byte0 or byte1 == param'.
  * player+0x1CC = 'targetable for grabs/specials' (1 in normal player states when record+0x67 == 0).
"""

ATK = {  # attack-type entries seen (table 0x086B8088, 8 bytes: kind, sub, 0, power 0x400)
    '0x086B8088': 'melee kind 0 sub 0 (can crit)',
    '0x086B8090': 'energy sub 1 (candy beam, multiplier 0)',
    '0x086B8098': 'melee sub 2 (normal punch)',
    '0x086B80A8': 'energy sub 4 (Energy Blast)',
    '0x086B80B0': 'energy sub 5 (Kamehameha)',
    '0x086B80C0': 'energy sub 7 (Masenko)',
}

PROJECTILES = {
    'buu_stretch': dict(
        what='Majin/Super Buu stretch-arm punch hitbox', builder='0x08028C04', vtable='0x0804CDD4',
        size_bytes=0x44, origin='boss + per-facing offset (0x08752D6C: down (9,16), up (15,-56), left (-24,-16), '
                                'right (48,-16); gfx 0x2B variant y -26) [DISASM]',
        length_px='8,12,...,32 (+4/frame, 7 frames), 32 for 1 frame, then 28..4 (-4/frame) [VERIFIED +0x34 series]',
        reach_px='origin + 32 along the facing: hit a player 80 px right of the boss centre [VERIFIED]',
        lifetime_frames='15 [VERIFIED]', speed='static (grows/shrinks)',
        damage=dict(attack_type='0x086B8088 (melee kind, sub 0)', multiplier=1024, stat='boss STR',
                    hits='once (hit at frame 7 of 15 on the player at 80 px) [VERIFIED]'),
        status='VERIFIED'),
    'buu_candy_beam': dict(
        what='Majin Buu candy beam', builder='0x080286D4', vtable='0x0804B3AC', size_bytes=0x50,
        origin='boss + 0x08752E28[facing] (right: +40,-26) [DISASM/VERIFIED]',
        length_px='8 -> 40 (+4/frame, 9 frames), then the tail retracts 4 px/frame [VERIFIED +0x40/+0x20]',
        reach_px='80 from the boss centre [VERIFIED]', lifetime_frames='19 [VERIFIED]',
        damage=dict(attack_type='0x086B8090 (energy sub 1)', multiplier=0, stat='POW', effect=
                    'no HP damage; player becomes candy for 300 frames (global 0x03002FE4 counts down 1/frame, '
                    'player controller vtable 0x0804BE60); IfGlobalFlag in Buu\'s trees tests it [VERIFIED]'),
        status='VERIFIED'),
    'kidbuu_beam': dict(
        what='Kid Buu Kamehameha (player beam class)', builder='0x080206B8', vtable='0x0804AEF8',
        level_def='0x0871DA68 (Kamehameha level 0: 32x32, mult 41, attack sub 5) [DISASM]',
        origin='boss + 0x0875925C[facing] (x,y): down (8,-2), up (12,-2), left (12,-2), right (4,-2) [DISASM]',
        growth='head advances ~3 px/frame: first hit at frame 10 (40 px), 12 (60), 16 (70), 19 (80), 21 (85); '
               'no hit at 90 px [VERIFIED]',
        reach_px='85-90 [VERIFIED]', thickness='hits at dy = +8 and -12 [VERIFIED]',
        lifetime_frames='fires at controller frame 8, beam ends with the state at ~frame 91-94 [VERIFIED]',
        damage=dict(attack_type='0x086B80B0 (energy sub 5)', multiplier=41, stat='boss POW',
                    hits='1-2 damage events per frame while overlapping (player i-frames zero most) [VERIFIED]'),
        status='VERIFIED'),
    'superbuu_ball': dict(
        what='Super Buu lobbed energy ball', builder='0x08021280', vtable='0x0804A33C',
        data='0x0872A6A8; launch 0x080214DA', size_bytes='>=0x60',
        origin='above the boss: (x+8, y-40) facing right [VERIFIED (433,393) for boss (425,433)]',
        charge_frames='60 at the origin [VERIFIED]',
        flight='aimed at the player position at launch; horizontal speed = distance/43 per frame '
               '(1.625 px/f for 70 px) ; height arc z0 = 32 px, vz0 = +2 px/f, gravity 1/8 px/f^2 (+0x50 += 32), '
               'lands after 43 frames [VERIFIED]',
        explosion='on landing becomes an explosion box 32 px wide growing to 64 px [VERIFIED bbox]',
        damage=dict(attack_type='0x086B80C0 (energy sub 7, Masenko)', multiplier=16589, stat='boss POW',
                    hits='1 (one-shot 135 HP Goku L60) [VERIFIED]'),
        status='VERIFIED'),
    'enemy_kiblast': dict(
        what='standard enemy ki blast (registered node KiBlast 0x0802B89D)', vtable='0x0804B454',
        speed_px_per_frame=3, lifetime_frames='~25-28 if nothing hit [VERIFIED]',
        damage=dict(attack_type='0x086B80A8 (energy sub 4)', multiplier=1024, stat='boss POW'),
        status='VERIFIED'),
}

_FIGHT = ('Majin/Super Buu fight loop (main state M, per frame, Selector order): 1) defence sub-state: after 90 '
          'frames in M, if the player attacks -> 20%/f Block state, if Buu is in the player beam line -> 80%/f '
          'Block (Block: guard while attacked, leaves at 5%/f otherwise); 2) player is candy (IfGlobalFlag) -> '
          'candy-chase state; 3) player > 120 px -> approach (ChasePlayer until within 80 px or 1%/f); 4) player '
          '> 50 px -> ranged roll; 5) else 100% close roll.  Every attack/move state returns to M.')

BOSSES = {
    0x1A: dict(
        name='Majin Buu (Giant Crater, with Babidi)', stats='HP 7320, STR 37, POW 40, END 55, resist melee '
        '64/128 (x0.5), energy 25/128 (x0.2) [DISASM enemies.json]', spawn='Giant Crater w8 m71 (224,232), '
        'cond 1; Babidi 0x1D and 0x04 in the same area [DISASM]',
        phases=[dict(name='idle', state='0xB4', trigger='player within 72 px (IfPlayerWithin2) -> 0xB5 [DISASM]'),
                dict(name='fight', state='0xB5', loop=_FIGHT),
                dict(name='pacified', state='0xC2', trigger='[0x0300302C] != 0 (IfBuuPacified): wander 0.5 '
                     'px/f (turn 2%/f) until it clears [DISASM; setter not found, INFERRED story script]')],
        attacks=[
            dict(name='ranged roll (player 50-120 px)', chances='90%/f: Selector[33% candy beam 0xBB, 50% '
                 'stretch 0xBC, ki blast 0xBD] = 33% / 33.5% / 33.5% [DISASM]',
                 pre='each first runs AlignWithPlayer 0.898 px/f (0.8%/f bail-out) [DISASM]'),
            dict(name='Candy beam', state='0xBB', node='CandyBeam 0x0802B3B5', startup='16 frames to spawn, hit '
                 'at frame 25 for 80 px [VERIFIED]', total='36 frames [VERIFIED]', projectile='buu_candy_beam',
                 follow='if the player became candy -> 0xBE: ChasePlayer 1.199 px/f; within 24 px -> 0xBF '
                        'EatCandy [VERIFIED chase 46 f from 80 px]'),
            dict(name='Eat (candy)', state='0xBF', node='EatCandy 0x0802B1FD', total='120 controller frames; '
                 'state 161 frames [VERIFIED]', damage='11 ticks (frames 24..104, every 8): player HP -= '
                 'min(POW/4 = 10, HP), never below 1; boss heals the same (+10 x 11 = 110 HP) [VERIFIED]'),
            dict(name='Stretch punch', state='0xBC', node='BuuStretchPunch 0x0802AD35', startup='spawn at '
                 'frame 64 (charge 8..55), hit at frame 71 at 80 px [VERIFIED]', total='81 frames [VERIFIED]',
                 projectile='buu_stretch'),
            dict(name='Ki blast', state='0xBD', node='KiBlast 0x0802B89D', total='34 frames [VERIFIED]',
                 projectile='enemy_kiblast'),
            dict(name='close roll (< 50 px)', chances='Selector[80% 0xB7 melee approach, 70% 0xB9 step-in '
                 '0.797 px/f (6%/f stop), 0xBA step 1 px/f] [DISASM]'),
            dict(name='Melee', state='0xB8 (from 0xB7 when IfPlayerInMeleeRange: 50%/f)', node='MeleeAttack + '
                 'Wait 1, 70% chance to chain another punch while in range', damage='attack 0x086B8098 '
                 '(melee sub 2), multiplier 1024, STR [VERIFIED]', timing='18-19 frames per punch [VERIFIED]')],
        movement='ChasePlayer 0.898 px/f (approach/melee), 1.199 px/f (candy chase); StepToward 0.797 / 1.0 '
                 'px/f [DISASM; chase step VERIFIED ~1.08 px/f average with pathfinding]',
        on_hit='aux of 0xC1 for main states: 10% -> 0xBA; after 90 f in state 80% -> Block + 0xC0; then the '
               'common reaction (state 0 aux) [DISASM]',
        verified=['candy beam -> candy 300 f -> chase -> eat 11x10 HP heal (bv_buu_majin.py 0x1a 80 0 260 0xbb)',
                  'stretch punch hit at 80 px frame 71 (bv_buu_series.py 0x1a 80 0 90 0xbc)',
                  'melee/ki-blast attack types and multipliers']),
    0x1B: dict(
        name='Majin Buu (Giant Crater, 2nd encounter)', stats='HP 14640 (double 0x1A), same STR/POW/END/resists',
        same_as='0x1A tree with: ranged roll 80%/f; close roll 50% melee-approach; melee 10%/f when in range '
                '(4% step-in), Wait 6 after a punch and 50% chain; chase speeds 0.699 (approach/melee), 1.0 '
                '(candy chase) [DISASM]', phases='idle 0x94 (72 px) -> fight 0x95; pacified 0xA2 [DISASM]'),
    0x1C: dict(
        name='Majin Buu (West City etc.)', stats='HP 45000, STR 42, POW 38, END 65, resists 64/25',
        spawn='w0 m12/m15/m21 (cond 0, story-placed) [DISASM]',
        same_as='0x1B tree WITHOUT candy beam / eat / pacify: ranged roll Selector[50% stretch 0xB0, ki blast '
                '0xB1] at 80%/f [DISASM]', phases='idle 0xA9 (72 px) -> fight 0xAA'),
    0x73: dict(
        name='Super Buu (Hyperbolic Time Chamber)', stats='HP 35330, STR 50, POW 65, END 55, resists 128/128',
        phases='idle 0x23A (72 px) -> fight 0x23B', same_as='fight loop with candy beam + eat; ranged roll '
        '80%/f Selector[33% candy 0x241, 40% stretch 0x242, ki 0x243] = 33/26.8/40.2%; close roll Selector[50% '
        'melee approach 0x23D (80%/f punch when in range, Wait 6, 50% chain; IfPlayerInFront -> step-in), 70% '
        'step-in, step 1.098 px/f]; chase 0.898 / 1.199 (melee) / 1.297 (candy) [DISASM]', no_power_up=True),
    0x74: dict(
        name='Super Buu (Giant Crater / Kami\'s Lookout) - two-phase', stats='as 0x73',
        phases=[dict(name='phase 1', states='0x249..', trigger='idle 0x248 until player within 110 px',
                     loop='as 0x73'),
                dict(name='absorb stance', state='0x255', trigger='IfPlayerBlastIncoming (player Energy Blast '
                     'flying at him within 240 px) -> 20%/f [DISASM]', behaviour='AbsorbStance 180 f: Energy '
                     'Blast hits do 0 HP (VERIFIED), 1st -> 0x256 (60 f), 2nd -> 0x257 (24 f) -> 0x258; melee '
                     'still hurts and triggers Block [VERIFIED]'),
                dict(name='power-up', state='0x259', trigger='Chance 0.8%/f inside the defence sub-state (i.e. '
                     '~0.8%/f during the fight loop) [DISASM; VERIFIED fired at frame 107 of a test]', behaviour=
                     'SuperBuuPowerUp: palette -> gfx 0x32, 55 frames, stats unchanged [VERIFIED]'),
                dict(name='phase 2', states='0x25B..0x26A', loop='faster: melee approach chase 1.5 px/f, 90%/f '
                     'punch, Wait 3, 70% chain; step-in 1.0; step 1.297; candy chase 2.0 px/f; AlignWithPlayer 1.0; '
                     'absorb stance at 100%/f when a blast comes; Block 40%/f (attacked) / 90%/f (beam line), '
                     'leaves Block at 2%/f [DISASM]', revert='absorbing 2 blasts in phase 2 (0x268/0x269) runs '
                     'RevertPalette and returns to phase-1 states [DISASM]')],
        verified=['absorb vs Energy Blast (bv_buu_reflect2.py)', 'power-up stats unchanged', 'phase-2 punch '
                  'raw 229995 one-shot Goku L60']),
    0x75: dict(name='Super Buu (no map spawn; script-created)', stats='as 0x73', same_as='0x74 (states '
               '0x26B..0x28D, identical structure) [DISASM]'),
    0x76: dict(
        name='Super Buu (Barren Wasteland w20 m5)', stats='HP 105300, STR 55, POW 75, END 65',
        phases=[dict(name='fight', states='0x28E (110 px) -> 0x28F', loop='no candy; ranged roll 80%/f: '
                     'Selector[33% ki 0x295, IfChildCount(0) -> 40% summon 0x296, ki 0x298] [DISASM]'),
                dict(name='summon', state='0x296', behaviour='7x SummonMinion (enemy 0x0D, 100 HP, 0 XP each), '
                     '86 frames apart (first at frame 39) [VERIFIED]; then 0x297 StaggerMinions + Wait 60 '
                     '[VERIFIED 62 f]; only when he has no living children [DISASM]',
                     minions='type 0x0D tree (fighters group): puff, wander 0.398 px/f, wait 120, then a '
                             'kamikaze node; +0x204 = stagger index [INFERRED]')]),
    0x77: dict(
        name='Super Buu (Barren Wasteland w20 m7, scripted 3-phase)', stats='HP 125000, STR 60, POW 85, END 65',
        spawn='record cond 0x20095 (story) at (214,255); 0x78 shares the map at (0,0) [DISASM]',
        phases=[dict(name='phase A', states='0x29B-0x29E', loop='GlideToWaypoint (60 f, 5 absolute waypoints), '
                     'face, Wait 12; then 50% glide again / 50% ThrowBigBall + Wait 180 [DISASM; glide VERIFIED]'),
                dict(name='phase advance', trigger='hit while in a ball state (0x29E/0x2A3/0x2A8 aux): the '
                     'glide on-hit handler accepts only when the player is in its anim slot 0x13 [DISASM; '
                     'INFERRED = bat the ball back], then cutscene script #474 / #475 / #476 (actors 52 = Super '
                     'Buu, 5) [DISASM]'),
                dict(name='phase B', states='0x2A1-0x2A3', loop='50% glide / 50% ball (no Wait 180)'),
                dict(name='phase C', states='0x2A6-0x2A8', loop='same'),
                dict(name='end', state='0x2AA Idle after script #476 [DISASM]')],
        attacks=[dict(name='Big ball', projectile='superbuu_ball', timing='60 f charge, 43 f flight, then '
                      'explosion [VERIFIED on 0x78]')],
        verified=['glide target table, 60-frame glide', 'off its own map the scripted fight removes the boss '
                  '~180 frames after the ball state [VERIFIED symptom, cause INFERRED]']),
    0x78: dict(
        name='Super Buu (Barren Wasteland m7 fighter, 0 XP)', stats='HP 125000, STR 60, POW 85, END 65',
        phases='idle 0x2AB (110 px) -> fight 0x2AC', same_as='fight loop without candy; ranged roll 80%/f: '
        'Selector[33% approach-then-ball 0x2B2 (chase 0.699 until within 80 px, then ThrowBigBall), '
        'IfChildCount(0) -> 33% summon 0x2B3 (7 minions), ki 0x2B5]; absorb stance 20%/f; on hit 10% -> fighter '
        'handler 0x0803044D [DISASM]', verified=['ball: 1 hit, attack 0x086B80C0 mult 16589 (bv_buu_ball.py)']),
    0x79: dict(
        name='Super Buu (Brain w21 m26)', stats='HP 100000, STR 63, POW 90, END 70',
        same_as='0x73-style loop with candy beam/eat + absorb stance (no power-up): states 0x2BC..0x2CD; '
                'melee approach also steps in when the player is within 20 px [DISASM]'),
    0x3C: dict(
        name='Kid Buu (Planet of the Kais)', stats='HP 125000, STR 66, POW 110, END 100, resist melee 96/128 '
        '(x0.75), energy 128', spawn='w19 m3 (45,319) cond 1 [DISASM]',
        phases=[dict(name='idle', state='0x11D', trigger='player within 100 px'),
                dict(name='phase 1', state='0x11E', loop='Selector: defence sub-state 0x127 (after 90 f, 20%/f '
                     'Block when attacked); player > 60 px: 2%/f -> split 0x128; within 50 px: 40%/f punch if in '
                     'melee range; within 70 px: 4%/f step-in 0.598; within 130 px: 3%/f beam 0x125; 1%/f align '
                     '0x126; > 90 px: 8%/f chase 0x121 (0.598 px/f, 12%/f punch); 0.8%/f back off 0x122 (0.797 '
                     'px/f); 12%/f chase; else stand+face [DISASM]'),
                dict(name='split', states='0x128 -> 0x129 -> 0x12A', behaviour='55-frame anim, then KidBuuSplit: '
                     'HP halved between him and a type-0x3D copy at the mirrored random offset (<=96 px each '
                     'axis) [VERIFIED]'),
                dict(name='phase 2 (two bodies)', state='0x12B', loop='same attack table (states 0x12B-0x133); '
                     'sub-state 0x135: only one Kid Buu body left -> back to 0x11E (can split again); 1800 frames '
                     'after the split -> 0x134 KidBuuMerge (remaining HP of the other body added back) '
                     '[DISASM; merge VERIFIED]', on_hit='10% teleport 0x131 (NinjaTeleport), 10% back off, '
                     'melee hit after 90 f: 40% Block [DISASM]')],
        attacks=[dict(name='Kamehameha', state='0x125/0x132', pre='AlignWithPlayer 0.797 px/f (0.5%/f bail), '
                      'SetCounter 0', node='KidBuuKamehameha 0x080331B5', projectile='kidbuu_beam',
                      timing='fires at frame 8, lasts to ~91 [VERIFIED]'),
                 dict(name='Punch', state='0x120/0x12D', node='MeleeAttack + Wait 6', damage='0x086B8098 x1024 '
                      'STR [DISASM]')],
        verified=['split 125000 -> 2 x 62500.5 (bv_buu_batch.py kid_split)', 'merge -> 125001', 'beam reach/'
                  'timing (bv_buu_reach.py 0x3c 0x125 ...)']),
    0x3D: dict(name='Kid Buu copy (created by the split, 0 XP)', stats='as 0x3C', phases='starts at 0x12A '
               '(phase 2 tree); identical behaviour, can merge into the other body [DISASM/VERIFIED]'),
    0x04: dict(name='Babidi (Giant Crater, script-placed at (0,0))', stats='HP 4300, POW 85, END 5, 0 XP',
               behaviour='BabidiGesture then Idle forever; no attacks [DISASM]'),
    0x19: dict(name='Babidi (West City)', stats='as 0x04', behaviour='MarkTime; BabidiHover (50 px up) + '
               'FollowMajinBuu(50 px) forever; untouchable by punches while hovering; no attacks [VERIFIED hover]'),
    0x1D: dict(name='Babidi (Giant Crater)', stats='as 0x04', behaviour='hover + follow Majin Buu; after 1200 '
               'frames 5%/f -> gesture (0xC6), then wander 0.297 px/f (turn 8%/f) until 300 frames + 5%/f -> back '
               '(0xC7->0xC5) [DISASM; hover VERIFIED]'),
}
