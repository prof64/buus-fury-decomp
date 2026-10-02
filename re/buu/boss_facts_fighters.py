"""Boss facts for the 'fighters' group (Broly, Dabura, Majin Vegeta, Vegeta, Babidi's fighters Pui Pui / Yakon /
Spopovich / Yamu, Mini-Yakon, helpers 0x0D/0x0E).  JSON-serialisable; every number tagged.

Conventions: frames at 60 fps; speeds px/frame; distances px (the AI compares squared px); chances are per
frame (p/65536); 'mult' = hit+4 multiplier (1024 = x1.0) fed to ComputeDamage 0x0800E9D8 with the attack type
(0x086B8088 table: sub 0 contact, 1 generic energy, 2 punch, 4 Energy Blast, 11 Final Flash); melee uses the
boss STR, energy its POW (formulas.json).  Generic nodes (ChasePlayer, MeleeAttack, KiBlast, Block ...) are
documented in enemy_ai.json; node names refer to ai_decode.REGISTRY + boss_nodes_fighters.REGISTRY.

Evidence scripts (emulator, real force-spawns via bv_fighters_lib.spawn2 = forcespawn method + 9999 HP):
bv_fighters_batch.py (forced states, logs in work/buu/fighters_runs/), bv_fighters_dabura*.py,
bv_fighters_broly2.py, bv_fighters_vuln.py, bv_fighters_yakon*.py, bv_fighters_duo.py, bv_fighters_death.py.
"""

V, D, I = 'VERIFIED', 'DISASM', 'INFERRED'

GENERIC_FIGHTER = dict(
    status=D,
    note='shared "fighter" tree (same as Majin Fighter, VERIFIED there by the earlier AI pass)',
    aggro='IfPlayerWithin2 100 px -> leave idle state (RunMotionController)',
    priority=[
        'SubState block-check: after 90 f in the state, if the player is attacking -> 20 %/f Block state '
        '(Block until the attack ends, 5 %/f give up)',
        'player within 50 px and in melee range: 12 %/f -> MeleeAttack + Wait 18',
        'player within 70 px: 4 %/f -> StepTowardPlayer 0.598 px/f (6 %/f stop)',
        'player farther than 90 px: 8 %/f -> Chase 0.598 px/f (2 %/f stop; 12 %/f melee if in range)',
        '0.8 %/f -> StepAwayFromPlayer 0.797 px/f while within 80 px (0.4 %/f stop)',
        '12 %/f -> Chase', 'else StandStill + FacePlayer'],
)

BOSSES = {
    0x0A: dict(
        name='Broly (Volcano Summit)', hp=20240, exp=30000, STR=32, POW=27, END=70, resist=(128, 128),
        flinch='pushes hit-stun state 2 on every hit (VERIFIED: punches every 20 f keep him in state 2); table '
               'byte says no knockback',
        initial_state=0x33,
        spawn=dict(area='Volcano Summit (world 15, map 32)', pos=(224, 200), gfx=38, record='0x0808BD6C',
                   condition_word='0x0002006F (inline; story-flag gate, INFERRED)',
                   defeat_script='0x0879321C (record+0x14 -> 0x0800424F run_script; fires on death, VERIFIED)'),
        phases=[
            dict(phase=1, states='0x33-0x3C, 0x41', status=D,
                 enter='state 0x33: BrolySpawnRocks (3 x 32x32 rocks at (200,136), (112,272), (296,240)) -> 0x34'),
            dict(phase=2, trigger='HP < 10 % (IfHpBelow 0x1999 checked every frame via SubState 0x41)',
                 states='0x43-0x50 (identical to type 0x0B)', status=V,
                 evidence='HP set to 9 % -> GotoState 0x43 on the first frame'),
        ],
        attacks=[
            dict(state=0x37, name='punch', trigger='player in melee range in state 0x36: 40 %/f', node='MeleeAttack',
                 hit_frame=0, mult=1024, atype='0x086B8098 melee', recovery='Wait 6',
                 chain='20 %/f re-enter 0x37 if still in range (40 % in phase 2)', status=V),
            dict(state=0x3C, name='charged punch', trigger='player in melee range in state 0x36: 40 %/f',
                 node='ChargedPunch', windup_frames=140, hit_frame=140, mult=1536, state_frames=149,
                 hitbox='rect adjacent to the facing side (dominant axis)', invulnerable='from frame 140 (obj+0x1CC=0)',
                 status=V),
            dict(state=0x3A, name='ki blast', trigger='player farther than 50 px: 15 %/f', node='AlignWithPlayer '
                 '0.699 px/f then KiBlast', projectile='enemy KiBlast (regular group), energy sub 4, mult 1024; '
                 'spawned ~15 f after the shot starts', status=V),
            dict(state=0x3B, name='block', trigger='after 60 f in a state: player attacking or Broly in the player '
                 'beam line: 20 %/f', node='Block', exit='player stops attacking, or 5 %/f', status=D),
        ],
        attacks_phase2=[
            dict(state=0x46, name='punch', mult=1024, chain='40 %/f', status=V),
            dict(state=0x4B, name='charged punch', mult=1536, windup_frames=140, state_frames=149, status=V),
            dict(state=0x49, name='ki blast', trigger='farther than 50 px: 15 %/f', align_speed=1.098, status=V),
            dict(state=0x4C, name='jump smash', trigger='farther than 50 px and no ki blast: 5 %/f',
                 node='JumpSmash', timeline='crouch f0-7, rise f8-55 (off screen), home f56-199 at 1 px/f per axis '
                 'toward the player, descend f200-247, land f248', landing_hit=dict(frame=248, mult=1024,
                 atype='0x086B8098 melee', hitbox='rect on the side facing the player (IfMeleeHit)'),
                 after_landing='state 0x4E: finish anim (~11 f) -> 0x43; if the landing destroyed a rock (2nd '
                 'landing on the same rock) -> state 0x4D: stunned 120 f (IfCooldownOver 120) -> 0x4F get-up',
                 status=V),
            dict(state=0x4A, name='block', trigger='after 90 f in a state: 20 %/f if attacked / in beam line', status=D),
        ],
        movement=dict(phase1=dict(step=0.797, chase=0.797), phase2=dict(step=1.098, chase=1.098),
                      note='StepToward/ChaseThroughRocks: rocks are passable for Broly only (msg 0x22)',
                      choice='state 0x34/0x43: farther 120 px -> chase state; else 70 %/f approach, 70 %/f step, '
                             'else wander-step; 0.8 %/f re-decide', status=D),
        specials=['arena rocks: 3 solid 32x32 blocks; Broly\'s jump landing cracks then destroys a rock; landing on '
                  'a rock that breaks stuns him 120 f (the intended opening)', 'no healing, no teleports'],
        verified=['charged punch hit 140 f after entry, mult 1536 (bv_fighters_batch broly_free/broly2_free)',
                  'phase change at HP < 10 % (broly_hp10)', 'jump smash timeline 248 f + landing hit x1.0 '
                  '(bv_fighters_broly2.py, donor gfx 0x0A)', 'ki blast energy sub 4 x1.0 (broly_3a)',
                  'no damage gate: punches land in any state (bv_fighters_vuln.py)',
                  'defeat script fires on death (bv_fighters_death.py)'],
    ),
    0x0B: dict(
        name='Broly (phase-2 type)', hp=20240, exp=30000, STR=35, POW=30, END=75, resist=(128, 128),
        initial_state=0x42, note='starts directly in the phase-2 tree (0x42 -> 0x43); has no spawn record of its own '
        '(script-created, INFERRED); no rocks are spawned by this type', same_as='0x0A phase 2', status=V),
    0x12: dict(
        name='Dabura', hp=10000, exp=7850, STR=22, POW=5, END=51, resist=(128, 128),
        flinch='hit-stun pushes state 2; every push increments obj+0x1E4 (VERIFIED: 3 hits -> counter 3)',
        initial_state=0x6E,
        spawn=dict(area='Demon World (8, 68)', pos=(144, 200), gfx=73, defeat_script='0x0878F33C'),
        phases=[
            dict(name='melee', states='0x6E-0x77', status=D,
                 desc='0x6E: MarkTime, counter=0 -> 0x6F priority: melee range 20 %/f -> 0x71 punch (Wait 6); within '
                      '70 px 4 %/f step 0.598; farther 90 px 8 %/f chase 0.598; 2 %/f AlignWithPlayer 0.699; 0.8 %/f '
                      'back off 0.797; 12 %/f chase; else stand/face. SubState 0x77 each frame: player petrified -> '
                      '0x7D; after 90 f: player attacking / beam line -> 0x75 Block (5 %/f -> 0x76 Wait 60); '
                      'counter == 3 -> 0x78'),
            dict(name='dark sphere', trigger='3rd hit-stun (obj+0x1E4 == 3)', states='0x78 -> 0x79', status=V,
                 desc='0x78 anim 0x12; 0x79 DarkSphere: sphere spawned (see PROJECTILES dabura_sphere). Sphere '
                      'destroyed (byte 0x030032E4 = 1) -> 0x80: anim 21 + Wait 300 (stunned 300 f, VULNERABLE) -> '
                      '0x6E; sphere expired (= 3, after ~444 f) -> 0x7A'),
            dict(name='spit barrage', states='0x7A-0x7C', status=V,
                 desc='0x7A anim, counter=0 -> 0x7B MoveToPoint(1.0 px/f -> map (192,80)), anim 0x13, Wait 180 -> '
                      '0x7C x10: AimAtPlayer, ParallelAny(Wait 42 | player petrified -> 0x7D), StoneSpit, '
                      'IncCounter; after 10 spits -> 0x6E'),
            dict(name='punish petrified player', states='0x7D-0x7F', status=V,
                 desc='0x7D Chase 1.0 px/f until in melee range -> 0x7E; 0x7E: player no longer petrified -> 0x6E; '
                      'in range -> 0x7F MeleeAttack + Wait 24 (one punch per 42 f observed); else chase 0.199 px/f'),
        ],
        attacks=[
            dict(name='punch', state=0x71, mult=1024, recovery='Wait 6', status=V),
            dict(name='stone spit', state=0x7C, projectile='dabura_spit', cadence='aim + 42 f + spit, x10', status=V),
            dict(name='dark sphere', state=0x79, projectile='dabura_sphere', status=V),
        ],
        verified=['sphere: grows 315 f, global 0->2 at f339, ->3 at f464 when not shrunk (bv_fighters_dabura.py)',
                  'spit: 2.0 px/f, energy sub 1 x1.0, petrifies the player ~297 f (bv_fighters_dabura2.py)',
                  'counter increments per hit-stun, sphere at 3 (bv_fighters_dabura3.py)'],
        gaps=['MoveToPoint (192,80) is unreachable on the test map, so the barrage start was forced'],
    ),
    0x46: dict(
        name='Majin Vegeta', hp=8500, exp=7850, STR=22, POW=25, END=36, resist=(128, 128), initial_state=0x15C,
        spawn=dict(area='Rocky Canyon (8, 70)', pos=(274, 303), gfx=282,
                   note='4 spawn records with different defeat scripts 0x0878E4D0 / 0x0878F534 / 0x0878F87C / '
                        '0x0878FE10 (story variants, INFERRED)'),
        phases=[dict(phase=1, note='single phase; no HP thresholds', status=D)],
        tree='generic fighter (aggro 100 px) plus: farther than 50 px 4 %/f -> 0x160 Align 0.598 + KiBlast; '
             'farther than 50 px 4 %/f -> 0x161 Align 0.598 + FinalFlash; after 90 f block check (attacked: 20 %/f)',
        attacks=[
            dict(name='punch', state=0x15F, mult=1024, recovery='Wait 18', status=V),
            dict(name='ki blast', state=0x160, projectile='enemy KiBlast, energy sub 4, x1.0', status=V),
            dict(name='Final Flash', state=0x161, projectile='final_flash_beam', fire_frame=24, state_frames=181,
                 observed='155 hits (one per frame) x328 energy sub 11 on a player standing in the beam', status=V),
        ],
        movement=dict(step=0.598, chase=0.598, back_off=0.797, align=0.598, status=D),
    ),
    0x88: dict(name='Vegeta (Gravity Chamber)', hp=5, initial_state=0x30F, status=D,
               behaviour='aggro 100 px; stands facing the player; player within 60 px -> 0x311: ParallelAll('
                         'StepAwayFromPlayer 2.0 px/f + InlineScript (natives 0x38, 0x3C x3, 0x47(0,282), '
                         '0x45(282,152,176,60), 0x3A) | player farther than 80 px -> back); no attacks (training NPC)'),
    0x89: dict(name='Vegeta (Barren Wasteland NPC)', hp=1, initial_state=0x313, status=V,
               behaviour='WanderToRandomPoint (x 96..391, y 112..351) then Wait 60, loop (~176 f per cycle observed)'),
    0x60: dict(name='Pui Pui', hp=50, exp=3170, STR=15, POW=15, END=10, initial_state=0x1CC, tree='generic fighter',
               note='50 HP in the type table (dies in a few hits)', status=V),
    0x91: dict(
        name='Yakon', hp=1725, exp=5300, STR=20, POW=68, END=41, resist=(7, 128),
        resist_note='melee resist 7/128 = 5.5 % of melee damage: effectively energy-only (type table, '
                    'enemies.json)', initial_state=0x31C,
        spawn=dict(area="Yakon's Planet (8, 40)", pos=(205, 96), gfx=300, defeat_script='0x0878EAE4'),
        loop=[
            '0x31C MarkTime -> 0x31D: Wander (4 %/f turn, 0.797 px/f) / 0x31E: MoveToPoint 0.797 px/f to map (216,96), '
            'alternating 4 %/f; SubState 0x320 each frame: after 300 f cooldown, 0.7 %/f -> 0x321',
            '0x321 StandStill, Wait 30, chase 1.398 px/f until in melee range -> 0x322',
            '0x322 combo: 6 x (chase 1.398 until in range, MeleeAttack x1.0); observed 6 hits ~17-32 f apart over '
            '~161-175 f; then anim 0x13, counter = 0 -> 0x323',
            '0x323 Wait 60; counter == 0 (not hit during the wait) -> 0x326 beam, else -> 0x324',
            '0x326 MouthBeamDown: beam straight down from (x+32, y-16), ~253 f of x328 energy sub 11 hits; state '
            '~271 f -> 0x324',
            '0x324 MoveToPoint 0.797 px/f to map (216,96) -> 0x325: anim, SummonMiniYakons (5 x type 0x92 at y 96, x 192..272 '
            'step 20), WaitSummonsDead (stands until all 5 die), anim -> 0x31C'],
        attacks=[dict(name='combo punch', state=0x322, hits=6, mult=1024, chase_speed=1.398, status=V),
                 dict(name='mouth beam', state=0x326, projectile='final_flash_beam', direction='down', status=V),
                 dict(name='summon', state=0x325, count=5, type=0x92, status=V)],
        verified=['combo 6 hits (yakon_free)', 'beam hits x328 sub 11 for ~253 f (bv_fighters_yakon2.py)',
                  '5 Mini-Yakons spawned at y 96 (bv_fighters_yakon.py)'],
    ),
    0x92: dict(name='Mini-Yakon', hp=850, exp=950, STR=20, POW=16, END=38, initial_state=0x327, status=V,
               behaviour='loop: Hop (32 f cycle) then 10 %/f -> 0x32A MiniYakonLunge (anim 4) + Wait 24 (~100 f '
                         'per cycle observed); damage only by touching: contact attack type 0x086B8088 (sub 0) x1.0',
               flinch='standard knockback + flinch'),
    0x6F: dict(
        name='Spopovich (with Yamu)', hp=1855, exp=450, STR=22, POW=0, END=25, initial_state=0x226,
        spawn=dict(area='World Tournament Stadium (7, 6)', pos=(392, 366), gfx=252, defeat_script='0x0878CBA8'),
        tree='generic fighter (no aggro check, WalkToPlayer 0.598 instead of chase) plus: player within 16 px: '
             '20 %/f -> 0x22E GrabWindup -> 0x22F BearHug',
        attacks=[dict(name='punch', mult=1024, recovery='Wait 18', status=V),
                 dict(name='bear hug', state=0x22F, effect='player frozen, held at (+8,-16); released when Yamu '
                      'finishes draining or after 5 D-pad presses (DISASM); no damage', status=V)],
        partner='Yamu 0x93 (class 0x130)', status=V),
    0x93: dict(
        name='Yamu (with Spopovich)', hp=1476, exp=350, STR=20, POW=0, END=23, initial_state=0x32C,
        spawn=dict(area='World Tournament Stadium (7, 6)', pos=(300, 366), gfx=303, defeat_script='0x0878CBE4'),
        tree='generic fighter (melee 20 %/f within 50 px, step 8 %/f, chase 2 %/f) plus SubState 0x333: partner '
             'Spopovich holding the player -> 0x334: Wait 60, EnergyDrain',
        attacks=[dict(name='energy drain', state=0x334, rate='1 EP per 8 f', amount='maxEP/4 (only if EP > '
                      'maxEP/4)', then='msg 9 -> Spopovich releases; 60 f', status=V)],
        verified=['grab + drain: 400 maxEP player drained 100 EP at 1/8 f, release at f1096 (bv_fighters_duo.py)'],
    ),
    0x6D: dict(name='Spopovich (225 HP variant)', hp=225, tree='generic fighter, WalkToPlayer 0.598', status=D),
    0x6E: dict(name='Spopovich (225 HP variant)', hp=225, tree='generic fighter, WalkToPlayer 0.598', status=D),
    0x70: dict(name='Spopovich (1855 HP, no grab)', hp=1855, tree='generic fighter, WalkToPlayer 0.598', status=D),
    0x94: dict(name='Yamu (no drain)', hp=1476, tree='generic fighter', status=D),
    0x0D: dict(name='swarm unit (gfx 44, unnamed)', hp=100, POW=46, initial_state=0x52, status=D,
               behaviour='SpawnPuff on entry; Wander 10 %/f 0.398 px/f; slot shuffle; MoveToFormationA/B (player + '
                         'table offsets 0x0877AE80 / 0x0877AECC); Wait 120; every state checks IfContactExplode: '
                         'touching anything = energy hit x4.0 (attack type 0x086B8090) then SelfDestruct'),
    0x0E: dict(name='freeze helper (gfx 45, unnamed)', hp=4450, STR=50, END=60, initial_state=0x58, status=D,
               behaviour='Wait 90, anim 0, FreezePlayer (player frozen while it plays anim 0x12), loop'),
    0x22: dict(name='inert helper', initial_state=0xE0, behaviour='Idle forever', status=D),
    0x72: dict(name='inert helper', initial_state=0x239, behaviour='Idle forever', status=D),
    0x7E: dict(name='inert helper', initial_state=0xE2, behaviour='Idle forever', status=D),
}

PROJECTILES = {
    'final_flash_beam': dict(
        ctor='0x08036FA2', vtable='0x0804AF24', size_bytes=0x48, users=['Majin Vegeta 0x161', 'Yakon 0x326'],
        shape='24 px wide (origin +-12), extends from the origin in the facing direction in 8 px steps until a '
              'wall tile (0x08009324 collision test)', status=D,
        damage=dict(atype='0x086B80E0 (energy, sub 11 Final Flash)', mult=328, stat='attacker POW',
                    rate='one hit per frame while the player overlaps', status=V),
        lifetime=dict(majin_vegeta='spawned at state frame 24, ~166 f (155 hits observed)',
                      yakon='~253 f of hits observed', status=V)),
    'dabura_spit': dict(
        ctor='0x0802E444', vtable='0x0804AC5C', size_bytes=0x54, base='enemy projectile base 0x08019B04',
        speed_px_per_frame=2.0, aim='at the player centre at spawn; velocity normalised on the dominant axis '
        '(major axis 2.0 px/f)', status=V,
        damage=dict(atype='0x086B8090 (energy, sub 1)', mult=1024, stat='Dabura POW (5)', status=V),
        effect='petrifies the player (stone anim); lasted ~297 f with no input (VERIFIED); while petrified Dabura '
               'chases and punches', lifetime='until it hits (39 f to cover ~78 px observed)'),
    'dabura_sphere': dict(
        ctor='0x0803AEA8', vtable='0x0804A884', size_bytes=0x20, shape='circle around Dabura (centre +2,-17)',
        status=V,
        growth='radius (1/256 px) starts 256 (1 px), +51 per frame (float 51.2) to 64 px (~315 f); then byte '
               '0x030032E4 = 2 and it expands +128/f (0.5 px/f) while fading (alpha (128-r)*50/64); at alpha 0 -> '
               'byte = 3 (expired, ~125 f later)',
        damage=dict(method='world broadcast msg 0x28 with the circle every frame (player inside takes it)',
                    atype='0x086B8098 (melee sub 2)', mult=1024, stat='Dabura STR', status=V),
        counterplay='each player ENERGY hit (attack kind 1) on the sphere while byte == 0 shrinks it by 102/256 px; '
                    'radius < 1 px -> byte = 1, 4 dust puffs, Dabura stunned 300 f (DISASM)'),
    'broly_rocks': dict(ctor='0x08036A44', vtable='0x0804AF50', size_bytes=0x3C, status=D,
                        shape='32x32 solid block (collision tiles), 3 at (200,136), (112,272), (296,240)',
                        states='intact -> cracked -> destroyed (def 0x0875F7AC / +0xC / +0x18), one step per Broly '
                               'landing; msg 0x22 toggles passability for Broly'),
}
