"""Behaviour-tree node registry for the Buu group (Majin Buu, Super Buu, Kid Buu, Babidi).

Same entry format as ai_decode.REGISTRY (ai_decode imports this automatically).  Extra keys:
  timing     : dict of frame counts (controller frames counted from the node's enter)
  projectile : key into boss_facts_buu.PROJECTILES
  on_hit     : what the def's word +4 (f1, run when the enemy is hit while this def is in the state's
               on-hit 'aux' tree) does
Status: VERIFIED = measured in the emulator (bv_buu_*.py), DISASM = read from code, INFERRED = guessed.

Evidence scripts: bv_buu_majin.py / bv_buu_series.py / bv_buu_batch.py / bv_buu_reach.py / bv_buu_ball.py /
bv_buu_reflect*.py (all use bv_buu_lib.setup = forcespawn + player warped in 150 px away, HP pinned).
"""

REGISTRY = {
    # ------------------------------------------------------------------ Majin Buu specials (also Super Buu)
    0x0802AD35: dict(
        name='BuuStretchPunch', kind='action', status='VERIFIED', projectile='buu_stretch',
        sem='needs player HP > 0 and player+0x1CC != 0 (player in a normal idle/walk/attack state, set by '
            'player state enters e.g. 0x08012FE6 when record+0x67 == 0); else finishes at once. Faces player, '
            'anim 0x13, controller 0x03003004: frames 0-7 wind-up, 8-55 charge (anim frames 1/2 alternate), '
            '56-71 release; frame 64 spawns the stretch-arm hitbox (builder 0x08028C04, vtable 0x0804CDD4) at '
            'the per-facing offset table 0x08752D6C (other table +0x28 when the sprite is gfx 0x2B); from '
            'frame 72 waits until that object is gone, then idle',
        timing=dict(spawn_frame=64, hit_frame_at_80px=71, arm_life=15, state_total_frames=81)),
    0x0802B3B5: dict(
        name='CandyBeam', kind='action', status='VERIFIED', projectile='buu_candy_beam',
        sem='same player-state precondition as BuuStretchPunch. Faces player, anim 0x12, controller '
            '0x0300304C: frame 16 spawns the candy beam (builder 0x080286D4, vtable 0x0804B3AC, offsets '
            '0x08752E28 / +0x24 for gfx 0x2B); from frame 32 waits for the beam to vanish. The beam hit uses '
            'attack type 0x086B8090 (energy, sub 1) with multiplier 0 -> 0 damage, and turns the player into '
            'candy: global [0x03002FE4] = 300 (frames, counts down 1/frame), player controller vtable 0x0804BE60',
        timing=dict(spawn_frame=16, hit_frame_at_80px=25, beam_life=19, state_total_frames=36)),
    0x0802B1FD: dict(
        name='EatCandy', kind='action', status='VERIFIED',
        sem='used when the player is candy and within 24 px. Faces player, anim 0x14, controller '
            '0x0300303C (120 frames): frame 0 stores the player position (0x03003044/48) and takes the player '
            'out of play (player controller -> 12-byte holder 0x0804C2CC); frames 24..111, every 8th frame: '
            'd = min(POW_boss >> 2, playerHP); active character HP -= d (never below 1: at 0 it is set to 1 '
            'and flag 0x03003038 = 1) and the boss heals d (enemy vfunc +0x60 with d<<10); frame 120 puts the '
            'player back at the stored position in the normal state',
        timing=dict(total=120, first_tick=24, tick_every=8, ticks=11)),
    0x0802B5B1: dict(
        name='IfBuuPacified', kind='cond', children=[0xC], status='DISASM',
        sem='true while the word [0x0300302C] != 0 (Majin Buu then wanders: state 0xC2/0xA2). The only '
            'literal writer found stores 0 (Babidi hover enter 0x0802AD88); the setter is probably a story '
            'script [INFERRED]. Never set during force-spawned fights (VERIFIED: stays 0)'),
    # ------------------------------------------------------------------ Super Buu
    0x08030157: dict(
        name='IfPlayerBlastIncoming', kind='cond', children=[0xC], status='DISASM',
        sem='object [0x03001B94] (the player\'s current projectile) exists, flies opposite to the boss facing '
            '(dir ^ 1 == boss dir), its vfunc +0x20 returns 4 (Energy Blast kind) and its bbox overlaps a '
            '240 px long, 8 px wider corridor in front of the boss'),
    0x0802FF97: dict(
        name='AbsorbStance', kind='action', status='VERIFIED',
        sem='anim 0x15, controller 0x03003128: 8 pose frames, then 0x0800EE78 parks the boss in the hold '
            'controller 0x03001BA4. Followed in the tree by Wait 180 + StandStill. The state\'s on-hit tree '
            '(aux 0x0870A378) catches Energy Blast hits (attack byte 4): no HP loss (VERIFIED: 29716 computed, '
            'HP unchanged), goes to state 0x256 (AbsorbPose + Wait 60); a 2nd Energy Blast there -> 0x257 '
            '(AbsorbPose2 + Wait 24) -> 0x258. Melee still hurts (VERIFIED -40 HP) and triggers Block'),
    0x0802FFD9: dict(
        name='AbsorbPose', kind='action', status='DISASM',
        sem='anim 0x15 with controller 0x03003130 (frame marks 4/12). on_hit (f1 0x0802FFDD): broadcasts '
            'world message 0x1D to all objects (0x08011C70; the player blast accepts it and vanishes) and '
            'returns 1 (hit absorbed)', on_hit='message 0x1D: absorb the blast'),
    0x0802FF51: dict(
        name='AbsorbPose2', kind='action', status='DISASM',
        sem='anim 0x15, controller 0x03003120. on_hit (f1 0x0802FF55): world message 0x1E, returns 1',
        on_hit='message 0x1E: absorb the 2nd blast'),
    0x0802FFF9: dict(
        name='AbsorbRecover', kind='action', status='DISASM',
        sem='anim 0x0E played once (controller 0x03001B20); used in state 0x258 together with a 180-frame '
            'cooldown'),
    0x0803017D: dict(
        name='RevertPalette', kind='action', status='DISASM',
        sem='if the palette (obj+0x38) is not gfx 0x31\'s, switch back to it (0x0800EECC); returns at once. '
            'Phase-2 Super Buu uses it after absorbing 2 blasts (state 0x269) -> back to the phase-1 states'),
    0x08030185: dict(
        name='SuperBuuPowerUp', kind='action', status='VERIFIED',
        sem='switch palette to gfx 0x32\'s (0x0800EECC) and play anim 0x0C once (idle controller '
            '0x03001B30). Stats/resists unchanged (VERIFIED 50/65/55/55, 128/128); the tree then jumps to '
            'the faster phase-2 state set',
        timing=dict(total=55)),
    0x0803923B: dict(
        name='IfChildCount', kind='cond', params=[(0xC, 'n', 'count')], children=[0x10], status='DISASM',
        sem='counts the boss\'s summoned children (linked list obj+0x1F0, next = child+0x1F8); true when '
            'count == n'),
    0x0803934B: dict(
        name='SummonMinion', kind='action', status='VERIFIED',
        sem='anim 0x12, controller 0x030032A0: frame 40 creates enemy type 0x0D (enemy def 0x087113E0: 100 HP, '
            '0 XP) at the per-facing offset table 0x0877AE58 ((8,12),(8,-12),(-12,8),(12,8)), links it as a '
            'child (child+0x1F4 = parent); frame 72 back to idle',
        timing=dict(spawn_frame=40, total=72)),
    0x08039473: dict(
        name='StaggerMinions', kind='action', status='VERIFIED',
        sem='r = rand(8); every child in list order gets child+0x204 = (r + k) & 7 for k = 1, 2, ...; '
            'returns 1 (staggers the minions\' kamikaze timing)'),
    0x0803C07B: dict(
        name='GlideToWaypoint', kind='action', status='VERIFIED',
        sem='Super Buu 0x77: waypoint index [0x030032EC] steps +-1 at random (rand(2)), wraps 0..4; target = '
            'table 0x0877B1BC (absolute map coords, 24.8: (224,104) (344,128) (120,128) (264,112) (144,144)); '
            'velocity = delta/60 per frame (60-frame glide, anim 0x0F, controller 0x03001B20, obj+8 = 0x24); '
            'boss+0x214 = index; faces the move direction. on_hit (f1 0x0803C08D): accepted (1) only when the '
            'player\'s current anim is slot 0x13 of its set, else 2 [INFERRED: player must be in a specific '
            'swing, i.e. bat the ball back]', on_hit='phase advance when accepted', timing=dict(glide=60)),
    0x0803BF0F: dict(
        name='ThrowBigBall', kind='action', status='VERIFIED', projectile='superbuu_ball',
        sem='anim 0x10, controller object 0x0804D6CC: on enter spawns the ball (builder 0x08021280, data '
            '0x0872A6A8, offsets 0x0877B180) 40 px above the boss; 60-frame charge; then aims at the player '
            '(0x080214DA) and throws; 32 frames later back to idle',
        timing=dict(charge=60, recovery=32)),
    0x0803C0BD: dict(
        name='SuperBuuAnim0C', kind='action', status='DISASM',
        sem='anim 0x0C played once (controller 0x03001B30); used between Super Buu 0x77 phases'),
    # ------------------------------------------------------------------ Kid Buu
    0x080331B5: dict(
        name='KidBuuKamehameha', kind='action', status='VERIFIED', projectile='kidbuu_beam',
        sem='faces player, anim 3, allocates motion controller vtable 0x0804C690: on enter creates the player '
            'beam class 0x080206B8 with level definition 0x0871DA68 (= Kamehameha level 0: 32x32, mult 41, '
            'attack sub 5) at offset table 0x0875925C; frame 8 fires (0x080207D2); ends after frame 90',
        timing=dict(fire_frame=8, first_hit_at_40px=10, end=91)),
    0x08032F7D: dict(
        name='KidBuuSplitAnim', kind='action', status='VERIFIED',
        sem='anim 0x0C played once with the idle controller (55 frames) before the split'),
    0x08032D2B: dict(
        name='KidBuuSplit', kind='action', status='VERIFIED',
        sem='up to 16 tries: d = (rand(192)-96, rand(192)-96); needs free ground at boss+d and boss-d '
            '(0x08009904); on success: effect 0x086FD9D0, boss moves to +d, HP<<10 = (HP<<10 + 1024) >> 1, and '
            'a new enemy type 0x3D (Kid Buu copy, 0 XP, starts in state 0x12A) appears at -d with the same HP. '
            'VERIFIED 125000 -> 62500.5 + 62500.5'),
    0x08032F6D: dict(
        name='KidBuuMerge', kind='action', status='VERIFIED',
        sem='finds the other Kid Buu (gfx 0x2F, list 0x0875922C); HP += other HP; moves to the midpoint '
            '(if free), two effects 0x086FD9D0, removes the other. VERIFIED 62500.5 + 62500.5 -> 125001'),
    0x080327FF: dict(
        name='IfLastKidBuu', kind='cond', children=[0xC], status='DISASM',
        sem='0x08011CCA(list 0x087591D8, 5, gfx 0x2F) == 1, i.e. only one Kid Buu body is left'),
    # ------------------------------------------------------------------ Babidi
    0x0802AEAB: dict(
        name='BabidiGesture', kind='action', status='DISASM',
        sem='if the sprite is in its default anim set: anim 0x13 looping (controller 0x03003024, frame '
            'counter wraps at 32); finished when the controller changes'),
    0x0802ADAF: dict(
        name='BabidiHover', kind='action', status='VERIFIED',
        sem='enter clears [0x0300302C]; height (s16 obj+0x176) rises 1 px/frame to -50 (50 px up) and '
            'stays; anim frames 1/2 alternate every 8 frames; never finishes. VERIFIED: height -50; player '
            'punches do not connect while hovering'),
    0x0802B591: dict(
        name='FollowMajinBuu', kind='action', params=[(0xC, 'dist', 'px')], status='DISASM',
        sem='on enter finds Majin Buu (gfx 0x2B in list 0x08752E74); each frame, if the dominant-axis distance '
            'between their bbox centres is > dist, steps 1 px/frame toward him (wall-checked per axis) and '
            'faces him; never finishes'),
}
