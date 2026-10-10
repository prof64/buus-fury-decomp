"""AI node registry, group 'others' (Pilaf's Guardian, Ninja Boss, Pharoah Totenhotep, Worm, Blister/Enzyme,
Blood Cell, Pikkon, Goten-story ...).  Same entry format as ai_decode.REGISTRY; imported by ai_decode.
Evidence: work/buu/others_dis.txt (bv_others_dis.py), emulator checks in bv_others_verify.py /
bv_others_enz3.py (results work/buu/bv_others_verify.json).

Common facts used below
* 'controller action': the node switches obj+0x14 to an IWRAM controller (0x0800ED18); the controller
  counts frames in obj+0x12 and returns to idle (vfunc +0x28) at its end frame; the node is finished when
  obj+0x14 is no longer that controller.
* spawn guards: projectile spawners need pool room 0x080116FC(pool) >= 0x80 (single) / 0x100 (burst).
* hit structs: {+0 dir byte, +4 mult (0x400 = 1.0), +8 attack type, +0xC attacker}; damage = ComputeDamage
  0x0800E9D8 with the attacker = owning enemy (STR for melee types, POW for energy types).
"""
L = 'list'

REGISTRY = {
    # ---------------------------------------------------------------- cooldown 2 (obj+0x1EC)
    0x0802AA81: dict(name='IfCooldown2Over', kind='cond', params=[(0xC, 'frames', 'frames')], children=[0x10],
                     status='DISASM',
                     sem='now(0x03000F94) - obj+0x1EC > frames (second cooldown timer, set by MarkTime2); '
                         'cond fn 0x0804EDB8'),
    0x0802AA2D: dict(name='MarkTime2', kind='action', status='DISASM',
                     sem='obj+0x1EC = frame counter; returns 1 (singleton 0x03003000, update 0x0804FFC8)'),
    # ---------------------------------------------------------------- ninjas
    0x0802C18F: dict(name='NinjaStarThrow', kind='action', status='VERIFIED', projectile='ninja_star',
                     timing=dict(fire_frame=8, end_frame=32, anim=0x13),
                     sem='face player; controller 0x03003074 (update 0x0802C08A): anim 0x13, at controller frame 8 '
                         'spawns one ninja star (builder 0x0802C790, spawn data 0x087548E8: per-facing offset '
                         '+8+8*dir) if pool room >= 0x80, sound 0x083192A8+0x2DC; frame 32 back to idle. '
                         'VERIFIED: star spawned 7 frames after the AI state entry, 1.75 px/frame'),
    0x0802C33D: dict(name='NinjaTeleport', kind='action', status='VERIFIED',
                     sem='instant teleport (update 0x0802C1DC): up to 16 tries of a random offset '
                         'dx,dy = rand(201)-100 px each, rejected unless max(|dx|,|dy|) >= 70; candidate tested '
                         'with enemy vfunc +0x54 (free placement); on success smoke puffs (effect 0x086FD9D0, '
                         'random 1024..2047 lifetime) at the old and the new bbox centre, then moves; '
                         'returns 1. VERIFIED: jump of (+59,+85) px observed'),
    0x0802C1D1: dict(name='NinjaDisguise', kind='action', status='DISASM',
                     sem='enter 0x0802C1B0: anim 0x12 with a random start frame (rand(anim length)), motion '
                         'controller = 0x03001B00 (static); update returns 1 at once -> the ninja sits '
                         'disguised as a prop until its next state'),
    0x0802C447: dict(name='NinjaReveal', kind='action', status='DISASM',
                     timing=dict(shake_frames=64),
                     sem='enter: controller object 0x0804BECC: every 2nd frame jitter position by '
                         'rand(3)-1 px on both axes around the stored spot for 64 frames, then back to idle; '
                         'update (0x0802C3D2) then spawns a smoke puff (0x086FD9D0) and returns 1'),
    # ---------------------------------------------------------------- mummies / pharoahs
    0x0802D1CD: dict(name='MummyWrapShot', kind='action', status='VERIFIED', projectile='mummy_wrap',
                     timing=dict(fire_frame=8, end_frame=24, anim=0x12),
                     sem='face player; controller 0x030030A8 (update 0x0802D0DC): anim 0x12, at frame 8 one '
                         'mummy wrap (builder 0x0802D79C variant 0 = straight, spawn data 0x08756A70 '
                         'per-facing offset) if pool room >= 0x80; frame 24 back to idle'),
    0x0802D319: dict(name='MummyWrapBurst', kind='action', status='VERIFIED', projectile='mummy_wrap',
                     timing=dict(fire_frame=8, end_frame=24, anim=0x12, count=8),
                     sem='controller 0x030030B0 (update 0x0802D214): at frame 8 (pool room >= 0x100) spawns 8 '
                         'wraps: for each facing 0..3 one straight (variant 0) and one diagonal (variant 1, '
                         'rotated 45 deg) from spawn data 0x08756AA0 offsets; frame 24 idle. VERIFIED: 8 '
                         'objects (vtable 0x0804A830) per burst, burst state period 26 frames'),
    # ---------------------------------------------------------------- heavy brutes (Guardian, Colossus ...)
    0x0802F919: dict(name='HeavyStrike', kind='action', status='VERIFIED',
                     timing=dict(windup_frames=60, hit_frame=58, total_frames=76, reach_px=60),
                     sem='enter 0x0802F74C: if the player exists and is active (player+0x1CC) -> enemy vfunc '
                         '+0x30(4) (wind-up action 4), timer 60. update 0x0802F8DA: at timer 0 performs the '
                         'strike 0x0802F77C: sound 0x083192A8+0x2E8, faces the player on the dominant axis, '
                         'builds a rect from its body edge towards the player and delivers a melee hit '
                         '{mult 0x400, attack type 0x086B8098 (melee sub 2)} through the player vfunc +0x24; '
                         'random hit sound on success; then vfunc +0x28(4) ends the action; finished when the '
                         'enemy controller is back to idle. VERIFIED (Pilaf\'s Guardian): hit 58 frames after '
                         'state entry, state left at 76; hits at dx <= 60 px (centre to centre), misses at 65'),
    # ---------------------------------------------------------------- Blister / Enzyme
    0x0802F5C5: dict(name='SpawnEnzymes', kind='action', status='VERIFIED',
                     timing=dict(interval_frames=120, max_spawns=8),
                     sem='Blister: enter timer=120, spawned=0 (obj+0x204). Every frame: timer-1; at 0 spawn '
                         'enemy type 0x18 (Enzyme, template 0x08757A4C) at the Blister position or +(16,-8) '
                         '(rand(2)) if the spot is free (0x08009904; else retry next frame), spawned+1, '
                         'timer=120. Stops for good after 8 spawns. Never finishes. VERIFIED: +1 Enzyme every '
                         '120 frames up to 8'),
    0x08037B97: dict(name='IfLatchOverlap', kind='cond', children=[0xC], status='VERIFIED',
                     sem='Enzyme: player body rect (0x0800FE78, rect table at obj+0x58 + 16*facing + 0x10) '
                         'shrunk by 6 px on the left and right overlaps the enzyme body rect. The enzyme rect '
                         'is non-empty (7x7 px) only when it faces down. VERIFIED: fires for the player 6-8 px '
                         'left of the enzyme, dy -8..+8'),
    0x0802995F: dict(name='LatchDrain', kind='action', status='VERIFIED',
                     timing=dict(drain_period_frames=64, drain_hp=1),
                     sem='Enzyme latched: anim 1, motion controller 0x0804C314 that keeps the enzyme glued '
                         'to the player; every 64 frames ((now & 63) == 0) calls player vfunc +0x5C '
                         '(0x08010BF2 HP loss) with 0x400 (= 1 HP); finished when the player is gone or HP <= 0. '
                         'VERIFIED: -1 HP every 64 frames, player held in place'),
    0x080363FB: dict(name='IfButtonMashed', kind='cond', params=[(0xC, 'presses', 'count'), (0x10, 'gap', 'frames')],
                     children=[0x14], status='DISASM',
                     sem='counts new presses of A/B/d-pad (mask 0xF3 of the pressed-edge word of input '
                         '[0x0875A738]); a gap longer than <gap> frames resets the count; true once the count '
                         'exceeds <presses>. Enzyme: >8 presses with gaps <= 15 frames'),
    # ---------------------------------------------------------------- Blood Cell
    0x0802C77B: dict(name='DriftContact', kind='action', status='VERIFIED',
                     sem='Blood Cell. create 0x0802C69C (node obj vtable 0x0804B640): random direction '
                         'a = rand(256)/256 turn, speed s = rand(3)+1; velocity = (cos a, sin a)*s*64/256 px '
                         '(sin table 0x087B6EEC) = 0.25/0.5/0.75 px/frame. update 0x0802C6EC: moves by the '
                         'velocity every frame (straight line), contact hit {melee 0x086B8098, mult 0x400} via '
                         'player vfunc +0x24; after a hit moves backwards for 16 frames. Never finishes. '
                         'VERIFIED: 0.5 px/frame drift, melee x1.0 contact hit'),
    # ---------------------------------------------------------------- Worm
    0x0802CCFF: dict(name='WormSpawnBody', kind='action', status='VERIFIED',
                     sem='Worm Head enter 0x0802CBD4: spawns 5 enemies of type 0x8F (Worm Body, template '
                         '0x08756A2C) at y-12, -24 ... -60 px and links them: obj+0x1F0 = next segment, '
                         'obj+0x1F4 = previous (head +0x1F4 = 0, tail +0x1F0 = 0). VERIFIED: 5 segments'),
    0x0802D62D: dict(name='WormHeadMove', kind='action', status='VERIFIED',
                     timing=dict(speed_px=1.0, turn_chance_per_frame=0.005, rehit_cooldown_frames=20),
                     sem='update 0x0802D4B0: every frame: with rand(200)==0 turn 90 deg (dir ^= 2, then 50% '
                         'dir ^= 1); step 1 px (table 0x08756B0C) in the facing; if the spot is blocked '
                         '(vfunc +0x54) try the next direction (dir+1 mod 4, up to 4); passes dir|0x80 to the '
                         'next segment (+0x1E4). Contact: melee hit 0x086B8098 x1.0 vs the player, then 20 '
                         'frames without contact damage. VERIFIED: 1.0 px/frame every frame'),
    0x0802D76D: dict(name='WormBodyFollow', kind='action', status='VERIFIED',
                     timing=dict(delay_frames=12, speed_px=1.0, rehit_cooldown_frames=20),
                     sem='update 0x0802D650: a 12-slot ring buffer of move commands received from the '
                         'previous segment (+0x1E4): each command is replayed 12 frames later as a 1 px step '
                         '(table 0x08756B38, no collision test) and forwarded to the next segment -> the body '
                         'traces the head path 12 px behind. Tail (no next) uses anim 0x12B. Contact melee x1.0, '
                         '20-frame re-hit cooldown. VERIFIED: segment copies a head turn ~12 frames later'),
    # ---------------------------------------------------------------- Pikkon
    0x0802E255: dict(name='EnergyBarrage', kind='action', status='VERIFIED', projectile='pikkon_barrage',
                     timing=dict(frames=48, anim=3),
                     sem='Pikkon: face player, sound 0x083192A8+0x2F4; controller 0x030030E0 (update '
                         '0x0802E146): anim 3, every frame until 48 spawns an energy shot object (0x080249E0 '
                         '-> builder 0x080246B4, vtable 0x0804BFCC, needs pool room >= 0x1400) at the facing '
                         'offset (0x08757324). VERIFIED: one energy hit (attack type 0x086B80A8, sub 4, mult 51 '
                         '= 0.05) per frame on a player 24 px away'),
    # ---------------------------------------------------------------- story Goten
    0x08033EE9: dict(name='TauntAnim', kind='action', status='DISASM',
                     sem='Goten (East District tag game): face player, anim 4 played once by controller '
                         '0x0804E030; finished at the end of the animation'),
    0x080338C5: dict(name='RunScript', kind='action', params=[(0xC, 'script', 'raw')], status='DISASM',
                     sem='queues the bytecode script whose header is <script> (event object vtable 0x08049264 '
                         'appended to the event list [0x0875931C]+0x610), returns 1. Goten: script #121 '
                         '(header 0x08783150)'),
}
