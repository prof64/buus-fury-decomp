"""Behaviour-tree node types used by regular (non-boss) enemies that were AUTO in enemy_ai.json, plus a
corrected AlignWithPlayer entry.  Same format as ai_decode.REGISTRY (auto-imported by ai_decode).

status: VERIFIED = emulator-checked (bv_regular_*.py), DISASM = read from code, INFERRED = partly guessed.
Projectile keys refer to boss_facts_regular.PROJECTILES.  Evidence/notes: work/buu/boss_regular.md,
work/buu/projectile_base.md."""
L = 'list'

REGISTRY = {
    # ---- corrected ----
    0x0801CFDB: dict(name='AlignWithPlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='VERIFIED',
                     sem='strafe on the minor axis (the axis with the smaller player offset) by exactly speed/256 px '
                         'per frame (pos += (0, +-speed) then collision try-move vfunc +0x54), facing up/down while '
                         'moving; when the minor offset < 3 px it faces the player on the major axis and returns 1. '
                         'Footstep sound for gfx 0x4E/0x11/0x4C at counter 0/16. VERIFIED bv_regular_align.py: '
                         'Mercenary 153 -> 153/256 px/f (53 frames), Majin/Shield Soldier 204 -> 204/256 px/f; the '
                         'earlier 0.5 px/f reading was contaminated by other movement'),
    # ---- shooters / throwers ----
    0x0802B6E5: dict(name='TurretFire', kind='action', status='VERIFIED', projectile='energy_wave',
                     timing='controller 0x0300305C: anim 0x14; frame 8 spawns the wave (pool room >= 0x80, spawn data '
                            '0x08752E78); anim frame 2 from f64; ends at f72',
                     sem='Atomizer / Ion Cannon / Laser Turret: face player (event 0x0804AE70), fire one energy wave '
                         '(class 0x08049C98) from the per-facing muzzle; finished when the controller ends (72 f)'),
    0x0802B701: dict(name='TurretOpen', kind='action', status='DISASM',
                     sem='turrets: anim 0x13 on the idle controller; finished when the anim frame (obj+0x11) reaches '
                         '3 (deploy / pop-up)'),
    0x0802B8B5: dict(name='TurretClose', kind='action', status='DISASM',
                     sem='turrets: anim 0x12 on the idle controller; finished at anim frame 3 (retract)'),
    0x0802F709: dict(name='CyborgShot', kind='action', status='VERIFIED', projectile='energy_wave',
                     timing='controller 0x03003108: frame 1 spawns the wave (spawn data 0x08757A80); afterwards the '
                            'controller holds until the wave object is gone AND >= 16 frames elapsed',
                     sem='Cyborg / Hyper Cyborg / Mega Cyborg: fire one energy wave straight ahead; blocks until it '
                         'disappears (hit, wall, off-screen or 240-frame cap)'),
    0x08026B2D: dict(name='BombThrow', kind='action', status='VERIFIED', projectile='bomb',
                     timing='controller (heap, vtable 0x0804B230): frame 16 throws (pool room >= 0x100, spawn data '
                            '0x0874CAD0); ends at f24',
                     sem='Bomber / Grenadier / Mad Bomber: face the player; aim offset d = player - enemy (px), the '
                         'component across the facing axis zeroed, each clamped to +-128; bomb velocity = '
                         'd * 0x71D >> 8 (1/256 px/f) so it lands on the target point after ~36 frames'),
    0x08024025: dict(name='FlameBreath', kind='action', status='VERIFIED', projectile='flame',
                     timing='controller 0x03002FA8: anim 0x12, 128 frames; from frame 4 one flame per frame '
                            '(124 flames) if pool room >= 0x1400; anim alternates 1/2',
                     sem='Annihilator / Death Machine / Destroyer: face player, sound, emit a continuous flame stream '
                         'from the per-facing nozzle offset (0x0872C358: down (46,10), up (16,-48), left (-6,-12), '
                         'right (74,-12)); finished when the controller ends'),
    0x08037E8D: dict(name='GrappleArm', kind='action', status='INFERRED', projectile='grapple_arm',
                     timing='controller 0x03003294: frames 0-23 anim flicker 0/1, 24-27 anim 2, 28 spawns the arm '
                            '(pool room >= 0x700, spawn data 0x0876CCB0), then holds until the arm is gone '
                            '(VERIFIED: spawn 28 frames after node entry)',
                     sem='Bio Mech family: face player and shoot an extending arm (class 0x0804B380)'),
    0x0803D34B: dict(name='LineUpForShot', kind='action', params=[(0xC, 'speed', 'speed256')], status='INFERRED',
                     sem='Bio Mech: strafe toward the player\'s row/column at speed (0x73 = 0.449 px/f) before '
                         'GrappleArm; finishes when lined up (VERIFIED: enemy y moved 32 px then fired)'),
    # ---- melee variants ----
    0x0803CD1F: dict(name='HeavyPunch', kind='action', status='VERIFIED',
                     timing='controller 0x030032F4: anim 4, frame 0 held for 48 frames (wind-up), then +1 anim frame '
                            'every 8 frames; the hit test runs when the counter reaches 48',
                     sem='Bruiser / Goon / Thug: face player, 48-frame wind-up, then one melee hit with the standard '
                         'enemy melee box (0x0802A0C4 pattern), attack type 0x086B8098, multiplier 0x600 (x1.5; '
                         'VERIFIED 27/27 hits mult 1536 exact); random punch sound'),
    0x08033D61: dict(name='WindupMelee', kind='action', status='VERIFIED',
                     timing='controller 0x03003214: anim 4, first 32 frames hold frame 0, then the anim advances; the '
                            'hit (0x08033BC4, standard melee box, 0x086B8098 x1.0) fires once when the anim reaches '
                            'frame 1',
                     sem='Bones / Lich / Skeleton: slow overhead swing (VERIFIED 12/12 hits mult 1024 exact)'),
    0x0803CB15: dict(name='BioMechPunch', kind='action', status='VERIFIED',
                     sem='Bio Mech family: face the player on the dominant axis, anim 4 on a per-instance controller '
                         '(vtable 0x0804C6B4) that performs one standard melee hit (0x086B8098 x1.0, VERIFIED 18/18 '
                         'exact); finished when the anim ends'),
    0x0803634F: dict(name='IfContactHit', kind='cond', children=[0xC], status='VERIFIED',
                     sem='every frame performs the standard melee hit test in front of the enemy (auto-facing, '
                         'attack type 0x086B8098, multiplier = node field +0xC << 10 = x1.0 for tanks); true (runs '
                         'the child) on the frame a hit lands. Tanks keep it running as a sub-state = contact/crush '
                         'damage every frame (VERIFIED 1950/1950 exact; the player\'s 24-frame hurt i-frames make most '
                         'of them 0)'),
    0x08037B2F: dict(name='LungeStep', kind='action', status='DISASM',
                     sem='Ronin / Samurai / Shogun: face the player and move 4 px/frame in the facing direction '
                         '(table 0x0876BA40) with collision, for the duration of anim 4; finished when the anim ends'),
    # ---- movement ----
    0x0802F435: dict(name='CirclePlayer', kind='action', params=[(0xC, 'speed', 'speed256')], status='DISASM',
                     sem='Cyborgs: face the player on the dominant axis and move perpendicular to the line to the '
                         'player (strafe / orbit) at speed (0x100 = 1 px/f); never finishes on its own'),
    0x08031229: dict(name='HoverDrift', kind='action', status='DISASM',
                     sem='Mechanoid family: velocity (node +4/+8, 1/256 px/f) accelerates toward the player by '
                         'int(38.4f)=38 per frame per axis, clamped to +-128 (0.5 px/f); hits received add an '
                         'impulse of +-0x400 along the hit direction; position integrated with collision'),
    0x080314A7: dict(name='BackStep', kind='action', params=[(0xC, 'k', 'raw')], status='DISASM',
                     sem='Mechanoid family: hop away from the facing direction for 48 frames: s = (rand(64)+96)*k>>7, '
                         'speed = 8*s/256 px/f backwards (k=16: 0.375-0.59 px/f) plus a random sideways drift of '
                         '(rand(32768)-16384)*s>>12; anim 1 on controller 0x03002B9C; collision-checked'),
    0x080315A5: dict(name='MechTransform', kind='action', status='DISASM',
                     sem='Mechanoid family: anim 0x14 on the idle controller; when it ends, swap the graphics set '
                         'between paired forms (gfx 0xBB<->0xBC, 0xE6<->0xE7, 0x85<->0x86); returns 1'),
    0x08032B7D: dict(name='TankDrive', kind='action', status='INFERRED',
                     sem='Juggernaut / Tank / War Machine: tank-style steering toward the player (turn helpers '
                         '0x08032948 / 0x0803287E / 0x0803282A, axis-aligned movement); never finishes (run in '
                         'ParallelAll with the fire check)'),
    0x080332E1: dict(name='IfPlayerInLineOfFire', kind='cond', children=[0xC], status='DISASM',
                     sem='true when the player is in front of the tank along its facing axis with the perpendicular '
                         'offset < 16 px (centre to centre)'),
    0x0802E2F3: dict(name='TankShoot', kind='action', status='VERIFIED', projectile='tank_shell',
                     timing='controller 0x030030EC: shell spawned at frame 0-2 (spawn data 0x08757350); the tree '
                            'then waits 12 frames',
                     sem='tanks: fire one shell straight ahead (class 0x0804A368)'),
    # ---- special mechanics ----
    0x08033B21: dict(name='VampireGrab', kind='action', status='VERIFIED',
                     sem='Vampire / Nosferatu / Vlad: player anim 9, faces the vampire, player controller -> '
                         '0x03001B00 (held), player pinned at the per-facing offset table 0x08759334; vampire anim '
                         '0x12. Every 16 frames (global frame counter & 15 == 0): player HP -= vampire POW (obj+0x1B2) '
                         'and the vampire heals the same (vfuncs +0x5C/+0x60, bypasses the damage formula). Escape: '
                         'each frame with any of A/B/d-pad held (mask 0xF3) counts 1 and stamps the time; >10 frames '
                         'without input resets the count; count > 8 releases. Ends when the player HP <= 0 or the '
                         'player leaves the held controller. VERIFIED bv_regular_vamp.py: -10 HP every 16 frames '
                         '(vampire POW 10) for 900+ frames with no input'),
    0x08018BCB: dict(name='KnockbackSlide', kind='action', params=[(0xC, 'frames', 'frames')], status='DISASM',
                     sem='move by the knockback velocity (0x08018A08: dir * ((rand(64)+96)*K)>>7) for <frames> '
                         'frames with wall collision; if the enemy was launched (obj+0x294/+0x298 set) and hits a '
                         'wall it emits a hit (attack type 0x086B80A0 melee-no-crit, mult int(409.6f)=409, its own '
                         'box) to the other actors'),
    0x0802CD7D: dict(name='RaiseShield', kind='action', status='DISASM',
                     sem='shield soldiers: swap graphics to the shield-up set (0xB0->0xAF, 0x102->0xB2, 0x56->0xAC); '
                         'returns 1'),
    0x0802CDE1: dict(name='LowerShield', kind='action', status='DISASM',
                     sem='shield soldiers: swap back to the shield-down set (0xAF->0xB0, 0xB2->0x102, 0xAC->0x56); '
                         'returns 1'),
    0x0802D49F: dict(name='ShieldBreak', kind='action', status='DISASM',
                     sem='shield soldiers: switch to the no-shield graphics (0xB0/0xAF->0xB1, 0x102/0xB2->0x100, '
                         '0x56/0xAC->0x55) and spawn a spark (0x0802A81C, anim 0x086FD9D0); returns 1. The tree '
                         'calls it when the counter obj+0x1E4 reaches 5 and then runs the plain fighter tree '
                         '(state 0x153)'),
    0x080264CD: dict(name='SelfMessage11', kind='action', status='INFERRED',
                     sem='sends message 0x11 to the enemy itself (vfunc +0x28) and returns 1 at once'),
}
