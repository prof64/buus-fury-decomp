"""Behaviour-tree node types owned by the 'fighters' boss group (Broly, Dabura, Majin Vegeta, Vegeta,
Yakon / Mini-Yakon, Spopovich + Yamu, helper types 0x0D/0x0E).  Same entry format as ai_decode.REGISTRY
(imported automatically by ai_decode).  Evidence: work/buu/fighters_nodes/<fn>.txt (disassembly, made by
bv_fighters_auto.py) and the emulator scripts bv_fighters_*.py (forced spawns, see boss_facts_fighters.py).

status: VERIFIED = timing/effect observed in the emulator; DISASM = read from code; INFERRED = name/purpose
guessed from animation ids / context."""

L = 'list'
REGISTRY = {
    # ---------------------------------------------------------------- Broly (0x0A / 0x0B)
    0x080369FB: dict(name='StepTowardPlayerThroughRocks', kind='action', params=[(0xC, 'speed', 'speed256')],
                     status='DISASM',
                     sem='StepTowardPlayer update (0x08018C3A) wrapped by world broadcast msg 0x22 = 1 before / 0 '
                         'after (0x08011C70): Broly\'s arena rocks (vt 0x0804AF50) clear their collision tiles '
                         'while he moves, so he walks through them; the player cannot'),
    0x080369B3: dict(name='ChasePlayerThroughRocks', kind='action', params=[(0xC, 'speed', 'speed256')],
                     status='DISASM',
                     sem='ChasePlayer (enter 0x0802A00C, update 0x0802A028, pathfinding) wrapped by msg 0x22 like '
                         '0x080369FB'),
    0x08035F81: dict(name='BrolySpawnRocks', kind='action', status='DISASM',
                     sem='enter: spawns 3 rock obstacles (ctor 0x08036A44, vt 0x0804AF50, 32x32 solid block, '
                         'collision box +16..+48) at map px (200,136), (112,272), (296,240) if the object pool has '
                         'room; returns 1 (Broly 0x0A initial state 0x33 only)'),
    0x08036193: dict(name='ChargedPunch', kind='action', status='VERIFIED',
                     timing=dict(windup_frames=140, hit_frame=140, state_frames=149),
                     sem='face player, anim 0x12, controller 0x03003278: frames 4..139 wind-up (own obj+0x1CC=1), '
                         'frame 140 one melee hit on the rect adjacent to the facing side (attack type 0x086B8098, '
                         'mult 0x600 = x1.5) if the player is hittable (player+0x1CC != 0); obj+0x1CC=0 from frame '
                         '140 (no hits taken during the strike); finished when the anim ends (~149 f). Emulator: '
                         'hit exactly 140 f after state entry, mult 1536; only lands if the player is inside the '
                         'rect at that frame (1 of 8 in a free fight)'),
    0x08035DCB: dict(name='JumpSmash', kind='action', status='VERIFIED',
                     timing=dict(crouch=(0, 7), rise=(8, 55), home=(56, 199), descend=(200, 247), land=248),
                     sem='anim 0x13, controller 0x0300325C: f0-7 crouch; f8-55 height obj+0x176 -= 8/f (leaves '
                         'the screen); f56-199 x and y each move 1 px/f toward the player centre (independent '
                         'axes); f200-247 height += 8/f; f248 three dust clouds (0x0802A81C, visual only), done. '
                         'Damage comes from the following IfMeleeHit (x1.0)'),
    0x08035EE3: dict(name='IfLandedOnRock', kind='cond', children=[0xC], status='DISASM',
                     sem='broadcast msg 0x21 with Broly\'s bbox to world objects; an overlapping rock goes intact -> '
                         'cracked (def 0x0875F7B8) -> destroyed (def 0x0875F7C4) per landing and answers true only '
                         'when destroyed; Broly is snapped onto the rock. True -> state 0x4D (120 f stun)'),
    0x08036303: dict(name='IfMeleeHit', kind='cond', children=[0xC], status='VERIFIED',
                     sem='one melee hit (attack type 0x086B8098, mult obj+0xC<<10 = x1.0) on the rect adjacent to '
                         'the side facing the player; child runs if it connected. Broly landing hit observed at '
                         'f248 of the jump, mult 1024'),
    0x08035E83: dict(name='FinishLandingAnim', kind='action', status='VERIFIED',
                     sem='if still in the jump anim: controller 0x0300326C plays the remaining frames from frame 6 '
                         '(4 f each), then idle; observed 0x4E -> 0x43 after 11 f'),
    0x08035BD1: dict(name='StunnedAnim', kind='action', status='INFERRED',
                     sem='anim 21 on the generic hold controller 0x03001B20 (used for the 120 f rock stun)'),
    0x08035E35: dict(name='RecoverAnim', kind='action', status='INFERRED',
                     sem='anim 20, controller 0x03003264; finished when the anim ends (get-up after the stun)'),
    # ---------------------------------------------------------------- Dabura (0x12)
    0x0803AE65: dict(name='IfPlayerPetrified', kind='cond', children=[0xC], status='VERIFIED',
                     sem='player obj+0x50 (current anim) == player type anim table +0xA8 (stone pose)'),
    0x0803AE83: dict(name='IfPlayerNotPetrified', kind='cond', children=[0xC], status='DISASM',
                     sem='negation of IfPlayerPetrified'),
    0x0802DE67: dict(name='PlayAnim12', kind='action', status='INFERRED',
                     sem='anim 0x12 on the hold controller 0x03001B20 (Dabura sphere wind-up)'),
    0x0803B2A9: dict(name='DarkSphere', kind='action', status='VERIFIED',
                     sem='enter: byte 0x030032E4 = 0, spawn the dark sphere (ctor 0x0803AEA8, vt 0x0804A884) at '
                         'Dabura centre + (2,-17); finished when 0x030032E4 becomes 1 (sphere destroyed) or 3 '
                         '(sphere expired); exit sets obj+0x1CC = 1. See boss_facts_fighters PROJECTILES '
                         'dabura_sphere'),
    0x0803B2BB: dict(name='IfSphereDestroyed', kind='cond', children=[0xC], status='DISASM',
                     sem='byte 0x030032E4 == 1 -> child (GotoState 0x80: 300 f stun)'),
    0x0802E33B: dict(name='PlayAnim12b', kind='action', status='INFERRED', sem='anim 0x12 on 0x03001B20'),
    0x0803AE5D: dict(name='StunnedAnim21', kind='action', status='INFERRED', sem='anim 21 on 0x03001B20'),
    0x0802E105: dict(name='MoveToPoint', kind='action',
                     params=[(0xC, 'speed', 'speed256'), (0x10, 'x', 'px'), (0x14, 'y', 'px')], status='DISASM',
                     sem='pathfind (0x0803B6EA / 0x0803BB48) to the fixed map point (x,y); finished within 8 px '
                         'on both axes. Dabura: speed 1.0 px/f to (192,80); Yakon: to his arena point'),
    0x0802DE75: dict(name='PlayAnim13', kind='action', status='INFERRED', sem='anim 0x13 on 0x03001B20'),
    0x0802DFCB: dict(name='AimAtPlayer', kind='action', status='DISASM',
                     sem='anim 20; controller 0x030030D0 turns to face the player on the dominant axis'),
    0x0802DF33: dict(name='StoneSpit', kind='action', status='VERIFIED', projectile='dabura_spit',
                     sem='anim 20, controller 0x030030C8: frame 0 spawns the petrifying spit (ctor 0x0802E444, '
                         'vt 0x0804AC5C) from mouth offset 0x087572C8[dir] aimed at the player centre (needs pool '
                         'room >= 0x100); frame 8 anim step'),
    # ---------------------------------------------------------------- Majin Vegeta (0x46)
    0x0803CE53: dict(name='FinalFlash', kind='action', status='VERIFIED', projectile='final_flash_beam',
                     timing=dict(fire_frame=24, state_frames=181),
                     sem='anim 0x13, controller vt 0x0804D7D0: frame 24 spawns the beam (0x08036FA2, vt 0x0804AF24) '
                         'from offset 0x0877C4D4[dir] if pool room >= 0x600; controller ends after frame 180'),
    # ---------------------------------------------------------------- Vegeta (0x88 / 0x89)
    0x0803333D: dict(name='InlineScript', kind='action', status='DISASM',
                     sem='runs the bytecode stored inline at def+0xC through the script VM (0x08004038) each update; '
                         'result = script return (1 = done). Vegeta 0x88: natives 0x38, 0x3C(0,12)x3, 0x47(0,282), '
                         '0x45(282,152,176,60), 0x3A (scripted escape / cutscene, INFERRED)'),
    0x0803BD65: dict(name='WanderToRandomPoint', kind='action', status='DISASM',
                     sem='enter: pick a reachable random map point x = 96+rand(296), y = 112+rand(240) at least '
                         '~9 px away; update pathfinds there (like MoveToPoint); Vegeta 0x89 NPC'),
    # ---------------------------------------------------------------- Yakon (0x91) / Mini-Yakon (0x92)
    0x0803A585: dict(name='PlayAnim13Yakon', kind='action', status='INFERRED',
                     sem='anim 0x13, controller 0x030032CC (post-combo pose)'),
    0x0803A77F: dict(name='MouthBeamDown', kind='action', status='VERIFIED', projectile='final_flash_beam',
                     sem='controller vt 0x0804D438: when the first anim cycle ends (frame == frames*8) fire the '
                         'beam class 0x08036FA2 straight down from (x+32, y-16) if pool room >= 0x600; controller '
                         'ends after frame 301. Emulator: beam hits x328 energy sub 11 every frame for ~253 f'),
    0x0803A61B: dict(name='PlayAnim12Yakon', kind='action', status='INFERRED', sem='anim 0x12, ctrl 0x030032D0'),
    0x0803A68F: dict(name='SummonMiniYakons', kind='action', status='VERIFIED',
                     sem='spawns 5 enemies from the spawn template 0x0877B0B0 (type 0x92 Mini-Yakon) at map px '
                         '(192,96), (212,96), (232,96), (252,96), (272,96) via enemy ctor 0x0800F68C'),
    0x0803A7EF: dict(name='WaitSummonsDead', kind='action', status='DISASM',
                     sem='finished when the world holds no object of class 0x12E (Mini-Yakon)'),
    0x0803A511: dict(name='PlayAnim12Yakon2', kind='action', status='INFERRED', sem='anim 0x12, ctrl 0x030032C8'),
    0x080353D1: dict(name='Hop', kind='action', status='DISASM',
                     sem='anim 14, controller 0x03003254: phase += 8/f (32 f cycle): 16 f rest, 16 f hop with height '
                         'obj+0x176 = -sin >> 13 (table 0x087B6EEC)'),
    0x08035205: dict(name='MiniYakonLunge', kind='action', status='INFERRED',
                     sem='if the player is alive and hittable: face player, anim 4 with its own controller '
                         '(vt 0x0804CD3C); an object vt 0x0804C098 appears 16 f in (no damage observed)'),
    # ---------------------------------------------------------------- Spopovich (0x6F) / Yamu (0x93)
    0x0802C4B3: dict(name='GrabWindup', kind='action', status='DISASM',
                     sem='face player, anim 0x12 on the play-once controller 0x0300308C; finished when idle again'),
    0x0802C673: dict(name='BearHug', kind='action', status='VERIFIED',
                     sem='player anim 0x14 + frozen controller 0x03001B00, held at Spopovich pos + (8,-16); partner '
                         '= first world object of class 0x130 (Yamu). Ends when the partner answers msg 9 (drain '
                         'finished) or after 5 D-pad presses (0x08040284 mask 0xF0, 20, 90) -> release, then 60 f. '
                         'Without a partner it ends at once (observed 2 f)'),
    0x0802F977: dict(name='IfPartnerHolding', kind='cond', children=[0xC], status='VERIFIED',
                     sem='first world object of class 0xFC (Spopovich) is on its hold controller 0x03001B30 with '
                         'anim 0x12 check (0x0800EEB0) -> child (Yamu GotoState 0x334); fired on the grab frame'),
    0x0802FC8B: dict(name='EnergyDrain', kind='action', status='VERIFIED',
                     sem='phase 0: pathfind to the held player; phase 1: player EP -1 every 8 f until it has lost '
                         'maxEP/4 (only if EP > maxEP/4), sound every 32 f; then msg 9 to the partner and 60 f '
                         'countdown. Observed: 400 maxEP player lost 100 EP at 1 per 8 f, released at f1096'),
    # ---------------------------------------------------------------- helper types 0x0D / 0x0E
    0x08039103: dict(name='IfContactExplode', kind='cond', children=[0xC], status='DISASM',
                     sem='each frame: energy hit (attack type 0x086B8090, mult 0x1000 = x4.0, dir = facing) with '
                         'the object bbox against the world (0x08011C0C); if anything was hit -> child (self-'
                         'destruct state 0x57)'),
    0x0803935D: dict(name='PlayAnim12Swarm', kind='action', status='INFERRED', sem='anim 0x12 on ctrl 0x03001B38'),
    0x0803943D: dict(name='WaitFormationSlot', kind='action', status='DISASM',
                     sem='singleton vt 0x0804CB00: finished once obj+0x204 (formation slot) != 0'),
    0x080393BF: dict(name='MoveToFormationA', kind='action', status='DISASM',
                     sem='MoveToPoint to player position + offset table 0x0877AE80[slot] (8 slots)'),
    0x0803942B: dict(name='MoveToFormationB', kind='action', status='DISASM',
                     sem='MoveToPoint to player position + offset table 0x0877AECC[slot]'),
    0x080189E9: dict(name='SelfDestruct', kind='action', status='DISASM',
                     sem='death effect (vfunc +0x74 with def+0xC) and removal from the world (0x08011944)'),
    0x0803084D: dict(name='PlayAnim0', kind='action', status='INFERRED',
                     sem='anim 0, controller 0x03003158 (frame thresholds 48 / 80)'),
    0x080305FF: dict(name='FreezePlayer', kind='action', status='DISASM',
                     sem='player anim 0 + frozen controller 0x03001B00 and input handler 0x03003144 while this '
                         'enemy plays anim 0x12 (ctrl 0x0300314C); exit restores the player input handler '
                         '0x03001C3C'),
}
