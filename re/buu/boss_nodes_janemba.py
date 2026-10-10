"""Behaviour-tree node types owned by the 'janemba' boss group (Janemba 0x33, Super Janemba 0x7A,
Mini-Janemba 0x38, Janemba Hands 0x36/0x39, body parts 0x34/0x35/0x37, object 0x8D).
Same entry format as ai_decode.REGISTRY (imported automatically by ai_decode).

Evidence: disassembly of the create fn / runtime vtable / motion controllers (DISASM) and emulator runs
(VERIFIED) with re/buu/bv_janemba_early.py (state timeline from creation), bv_janemba_super.py,
bv_janemba_shot.py, bv_janemba_vuln.py.  Frame numbers are 60 fps frames from the node's start."""

REGISTRY = {
    # ------------------------------------------------------------------ Janemba (0x33)
    0x08039D7B: dict(
        name='JanembaSpawnParts', kind='action', status='VERIFIED',
        sem='singleton 0x030032C4.  Creates the three body-part enemies from the spawn records at 0x0877AFDC '
            '(+0x20 each): type 0x34 (gfx 0x93), 0x35 (gfx 0x94), 0x37 (gfx 0x98) and links them '
            'boss+0x1F0 -> 0x37, 0x37+0x1F8 -> 0x34, 0x34+0x1F8 -> 0x35.  Measured offsets from the boss '
            'position: 0x34 (-12,-22), 0x35 (+44,-22), 0x37 (+16,-41) px.  Parts have 0 HP, Idle AI; 0x37 '
            'mirrors the body animation (node 0x0803D0FB).  Returns 1 the same frame.',
        timing=dict(parts_created_frame=2)),
    0x0803A2B7: dict(
        name='JanembaHandStrikeA', kind='action', status='VERIFIED',
        sem='Arm strike with part 0x34 (boss+0x1F0 -> +0x1F8; screen-left part).  Part gets anim 0x12 and '
            'controller 0x0804D738: on controller frame 16 a Janemba Hand enemy (type 0x36, gfx 0x96, spawn '
            'record 0x0877B050) is created at the PLAYER position + (4, 0) px, facing 2, with anim 0x13 and '
            'controller 0x0804C338 (rising hand): one hit, tested on hand frames 16..55 inclusive, box = the '
            'hand body box (0x0800FE78), attack type 0x086B8098 (melee sub 2, can crit), multiplier 0x400, '
            'attacker = the hand (type 0x36 STR 22).  The hand is removed when its vfunc+0x34 reports done.  '
            'Node finishes when the part animation ends.',
        timing=dict(hand_spawn_frame=16, hand_hit_window=[32, 71], first_player_hit_measured=33,
                    node_length_measured=57, status='VERIFIED (state 0xFE: 10 strikes in 573 f, hits every 57 f)'),
        projectile='janemba_hand_36'),
    0x0803A475: dict(
        name='JanembaHandStrikeB', kind='action', status='VERIFIED',
        sem='Same as JanembaHandStrikeA but drives part 0x35 (boss+0x1F0 -> +0x1F8 -> +0x1F8; screen-right '
            'part) through controller 0x0804D98C: hand created at the PLAYER position + (-20, 0) px, facing 3, '
            'hand anim 0x12.  Same hit window / damage.',
        timing=dict(hand_spawn_frame=16, hand_hit_window=[32, 71], node_length_measured=57),
        projectile='janemba_hand_36'),
    0x08039983: dict(
        name='JanembaAscend', kind='action', status='VERIFIED',
        sem='anim 0x13 + controller 0x030032B8 (vt 0x0804CC34): from controller frame 8 the height offset '
            'obj+0x176 of the body and all three parts is set to max(24 - 3*frame, -200) px (rises off the '
            'top of the screen at 3 px/frame).  Finishes when the controller frame counter exceeds 128.  '
            'While airborne the boss takes no damage (VERIFIED: 6 punches, 0 HP lost, +0x176 = -132).',
        timing=dict(length=129, rise_px_per_frame=3, max_height=-200)),
    0x08039BBB: dict(
        name='JanembaDescendSummon', kind='action', status='VERIFIED',
        sem='anim 0x13 + controller 0x0804D3C8: height starts at -200 and rises +3 px/frame back to 0 '
            '(67 f), then a 16-frame countdown; when it reaches 0: screen effect 0x0800797C(0x46, 0x1E), '
            'count the living Mini-Janembas (world message 5 with the gfx-0x99 def) and create (5 - count) '
            'Mini-Janembas (type 0x38, record 0x0877AF78) at the fixed map positions 0x0877AF98: '
            '(104,112) (120,136) (152,144) (184,136) (200,112) px - i.e. tops the swarm up to 5.  Finishes '
            'when the body animation ends.',
        timing=dict(ascend_plus_summon_state_length=230, minis_created_at=229)),
    0x080398C5: dict(
        name='WaitMinisDefeated', kind='action', status='VERIFIED',
        sem='singleton 0x030032B4.  Each frame sends world message 5 with the gfx-0x99 (Mini-Janemba) def '
            '(0x08011CCA) and returns 1 when no object answers, i.e. when every Mini-Janemba is dead.  '
            'VERIFIED: Janemba stayed in state 0x104 for 2000+ frames while minis lived, left it on the '
            'frame the last one was killed.'),
    0x0803D0FB: dict(
        name='PartMirrorAnim', kind='action', status='INFERRED',
        sem='Janemba part 0x37: each frame compares the linked boss (obj+0x1F4) animation pointer with '
            'two entries of its anim def (+0x78 / +0xA0) and switches its own animation to match '
            '(0x0803CF46 / 0x0803D046 / 0x0803CFC2).  Cosmetic; never finishes.'),
    # ------------------------------------------------------------------ Mini-Janemba (0x38)
    0x08034817: dict(
        name='MiniHopToward', kind='action', status='VERIFIED',
        sem='anim 0x12 + hop controller 0x0300322C (vt 0x0804D968): counter +8/frame, 32-frame cycle; '
            'airborne in the second half (counter 0x80..0xFF, height = -sin>>13 arc).  At each landing '
            '(counter 0) with chance 0x4000 (25%) it re-aims at the player diagonal quadrant (facing 0 '
            '(+,+), 1 (-,-), 2 (-,+), 3 (+,-), vectors 0x087593F8).  While airborne it moves 0.5 px/frame '
            'on both axes along that diagonal (0.25 px/frame average).  Never finishes (returns 0).  '
            'Measured: 6.5-7 px per 30 frames per axis.',
        timing=dict(cycle=32, airborne=16, speed_px_per_frame_airborne=0.5)),
    0x08034A0B: dict(
        name='MiniHopFlee', kind='action', status='DISASM',
        sem='Low-HP Mini-Janemba (state 0x111): same 32-frame hop, but at each landing the facing is set '
            'from the dominant axis and a normalised vector of length 0.5 px/frame from the mini TO the '
            'player is computed; while airborne the mini moves by MINUS that vector (flees).'),
    0x080345FD: dict(
        name='IfPlayerNearAndHittable', kind='cond', children=[0xC], status='DISASM',
        sem='true when the player object ([0x03001B94]) has its hittable word +0x1CC set and the centres of '
            'the two collision boxes (0x0800FE78) are within 47 px on both axes (|dx|,|dy| <= 47).'),
    0x08034EC1: dict(
        name='MiniRollAttack', kind='action', status='VERIFIED',
        sem='singleton 0x0300324C; anim 0x12 + controller 0x03003248 (vt 0x0804E170), counter +8/frame.  '
            'Counter 0x81..0xFF (frames 16-31): dash along its diagonal facing at 1.5 px/frame per axis '
            '(table 0x0875947C (+-3 px) * 0.5).  Counter 0x100 (frame 32): one area hit broadcast to the '
            'world (0x08011C0C kind 0) with box = own body box, attack type 0x086B8088 (melee sub 0), '
            'multiplier 0x400, attacker = the mini (STR 20); returns 1.  VERIFIED: player hit 32 frames '
            'after entering state 0x112 (then Wait 120).',
        timing=dict(windup=16, dash=[16, 31], hit_frame=32, dash_px_per_frame_per_axis=1.5)),
    # ------------------------------------------------------------------ Janemba Hand (0x39, HFIL fields)
    0x0803B4C9: dict(
        name='HandAmbush', kind='action', status='VERIFIED',
        sem='rand(2): teleport to the player position + (4,0) px facing 2, or + (-20,0) px facing 3; anim '
            '0x12 + controller 0x0804D714: one hit tested on frames 80..103 (box = own body, attack type '
            '0x086B8098 melee sub 2, multiplier 0x400, attacker = this hand, STR 35).  Finishes when the '
            'controller ends.  Measured: player hit 81 f after entering state 0x115, state lasts 106 f.',
        timing=dict(hit_window=[80, 103], length_measured=106)),
    0x0803B357: dict(
        name='HandSink', kind='action', status='VERIFIED',
        sem='waits for the current animation to end, then controller 0x0804DBA8 (sink into the ground); '
            'finishes when the hand leaves that controller.  State 0x117 (this + Wait 180) measured 287 f.',
        timing=dict(length_measured=107)),
    # ------------------------------------------------------------------ Super Janemba (0x7A)
    0x08034BA7: dict(
        name='SuperJanembaWave', kind='action', status='VERIFIED',
        sem='singleton 0x03003240: face the player (event 0x0804AE70), anim 0x13, controller 0x0300323C '
            '(vt 0x0804DBCC).  Controller frame 24: if pool room (0x080116FC) >= 0x800, spawn an energy '
            'wave (builder 0x080359D4) at bbox-origin + per-facing offset (0x0875943C+4: down (4,4), up '
            '(13,-24), left (-8,-8), right (24,-8)).  Frame 40: back to idle (node done).',
        timing=dict(spawn_frame=24, length=40, measured_spawn_after_state_entry=25),
        projectile='super_janemba_wave'),
    0x08034CE3: dict(
        name='TeleportNearPlayer', kind='action', status='VERIFIED',
        sem='singleton 0x03003244.  enter: save position at obj+0x208, side = rand(4), controller 0x0804D3EC '
            '(anim 0x12).  Controller frame 36: anim 0x14 and the boss is moved to the player position + '
            'side offset ((0,+32), (0,-32), (-32,0), (+32,0) px; wall-corrected with 0x08009816).  Finishes '
            'once the animation is no longer 0x11/0x13.  VERIFIED: jump on frame 36, landed (-32.8,-0.8) px '
            'from the player; state 0x2DB (this + FacePlayer + MeleeAttack) lasted 90 f.',
        timing=dict(teleport_frame=36, distance_px=32)),
    0x0803455D: dict(
        name='TeleportChain4', kind='action', status='VERIFIED',
        sem='singleton 0x03003224 with a persistent counter at 0x03003228.  Teleports (controller '
            '0x0804DF9C, same as TeleportNearPlayer) to side = counter & 3 (0 below, 1 above, 2 left, 3 '
            'right of the player, 32 px), counter += 1 (mod 12) after each; finishes after the teleport '
            'that makes counter % 4 == 0, i.e. 4 teleports per run.  VERIFIED: teleports every 72 f to '
            '(0,+31) (0,-33) (-38,-8) (+32,-1) px, then FacePlayer + MeleeAttack, loop 306 f.',
        timing=dict(teleport_every=72, teleports=4, loop_measured=306)),
    # ------------------------------------------------------------------ object 0x8D (map 0/50)
    0x08036F45: dict(
        name='IfPlayerOnTop', kind='cond', children=[0xC], status='DISASM',
        sem='true when the player position is within 16 px of this object on both axes (and a height '
            'test on obj+0x1A8, >>10).  Used by enemy type 0x8D (12 placements on map 0/50, 0 HP).'),
    0x08036E77: dict(
        name='TriggerSelf', kind='action', status='INFERRED',
        sem='singleton 0x03003280: calls the object\'s own vfunc +0x74 with 1 (the same call the boss '
            'death handler 0x0800A9B9 makes) and returns 1 - the object triggers / removes itself.'),
}
