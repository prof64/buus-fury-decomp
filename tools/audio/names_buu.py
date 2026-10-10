"""Names, usage and trigger evidence for Buu's Fury songs and sound effects (see AUDIO.md for the method).

confidence:
  high   - read from the area table / a native call site, or verified by breakpointing PlaySfx in the emulator
  medium - script bytecode context (warp target, chapter card, enemy ids next to the call)
  low    - inferred from the surrounding code only
  none   - no play site found
"""

# script ids below are entries of the script table 0x0879B704 (re/buu/scriptdis.py); a "warp" is native 0x73
SONGS = {
    1: dict(name="Babidi's Ship", confidence='high',
            context="Inside Babidi's Ship (8-18..8-69), Yakon's Planet (8-40), Demon World (8-68), Giant Crater (8-71)"),
    3: dict(name="Bulma's Airship (0-15)", confidence='medium', context='area 0-15, an unnamed map next to Bulma\'s Airship'),
    4: dict(name='Capsule Corporation', confidence='high',
            context="Capsule Corp interiors 4-21..4-31 (Lobby, Computer Room, Exhibit Hall, Gravity Chamber, Dr. Brief's Lab)"),
    5: dict(name='Musuka Circus', confidence='high', context='Musuka Circus 3-14..3-19'),
    6: dict(name='Town', confidence='high',
            context="Hercule City, Hercule's Mansion, Orange Star High School, West City, Capsule Corporation, Bulma's "
                    'House, Papaya Island; Hercule City 3-1..3-3 before story flags 26+27 (area music script); script #152 '
                    '(Chapter 8 "Janemba" card)', scripts=[152]),
    8: dict(name='Game over', confidence='high',
            context='the player\'s KO: the death script 0x08799D0C (started by the KO 0x0801088A) plays it 200 frames later',
            evidence='lockstep: every *_ko scenario (tools/battle/LOCKSTEP.md), PlaySong breakpoint'),
    9: dict(name="Goku's House", confidence='high',
            context="Goku's House 2-1..2-5, East District 439 (2-8), Nataday Village (15-1, 15-33), area 0-16"),
    12: dict(name='Airship', confidence='high', context='Airship / Airship Interior / Control Room 12-1..12-28'),
    15: dict(name='Snakeway (Other World)', confidence='high',
             context="Snakeway, Yemma's Office, Grand Kai's Mansion / Planet (1-10, 1-11), Cave of the Ancients; script #38 "
                     '(Chapter 1 "The Other World" card), #25', scripts=[25, 38]),
    17: dict(name='Z-Mart', confidence='high', context='Z-Mart 0-91, 0-92, 0-94; script #17 (Chapter 7 "Majin Buu" card), #119',
             scripts=[17, 119]),
    18: dict(name='HFIL', confidence='high', context='HFIL 18-1..18-14'),
    19: dict(name='Northern Wilderness / Volcano', confidence='high',
             context='Northern Wilderness 15-2..15-18, Volcano 15-19..15-31, Volcano Summit (15-32, Broly), Diablo Desert '
                     'Express 11-17..11-24, area 0-90'),
    20: dict(name='Inside Buu', confidence='high', context='Stomach 21-1..21-14, Intestines 21-15..21-25, Brain 21-26'),
    21: dict(name="Kami's Lookout", confidence='high', context="Kami's Lookout / Palace 6-1..6-5, 6-13..6-20, Hyperbolic Time Chamber 6-6..6-12"),
    23: dict(name='Scripted scene (script #460)', confidence='low', context='script #460 (also plays SFX 6 / 7)', scripts=[460]),
    24: dict(name='Fukurou Forest / Ninja Stronghold', confidence='high',
             context='Fukurou Forest 13-1..13-3, Ninja Stronghold 13-32..13-53, outside Kyodai Pyramid 14-1, 14-2'),
    25: dict(name='Other World Stadium', confidence='high',
             context='Other World Stadium (1-20); script #453 (warp to HFIL 18-90)', scripts=[453]),
    26: dict(name="Grand Kai's Planet", confidence='high', context="Grand Kai's Planet 1-12..1-15"),
    27: dict(name="Pilaf's Castle / Barren Wasteland", confidence='high',
             context="Pilaf's Castle 10-1..10-31, Barren Wasteland 20-1..20-10 (Super Buu / Kid Buu)"),
    28: dict(name='East District 439 / Southwest Forest', confidence='high',
             context="East District 439 2-9..2-90, Grandpa Gohan's Hut, Southwest Forest 8-1..8-16, Babidi's Ship entrance (8-17)"),
    29: dict(name="Thieves Den / Hercule City (after Buu's attack)", confidence='high',
             context='Thieves Den 9-1..9-8, Lighthouse 9-9 / 9-17; Hercule City 3-1..3-3 once story flags 26+27 are set (area '
                     'music script); scripts #125, #131, #168', scripts=[125, 131, 168]),
    30: dict(name='Diablo Desert / Kyodai Pyramid', confidence='high',
             context="Diablo Desert 11-1..11-16, Kyodai Pyramid 14-3..14-23, Totenhotep's Chamber 14-24, 14-25"),
    31: dict(name='Heaven', confidence='high', context='Heaven (22-1)'),
    34: dict(name='Hideout', confidence='high',
             context='Rogue Submarine (0-52), Stolen Airship (0-55), Z-Mart back rooms (0-93, 0-95), Thieves Den 9-10..9-16'),
    35: dict(name='Planet of the Kais', confidence='high',
             context='Planet of the Kais 19-1..19-7, area 0-9; script #456 (warp to 19-3)', scripts=[456]),
    36: dict(name='Story scene (script #4)', confidence='low',
             context='script #4, started from native code 0x08005DEC (new-game / story setup)', scripts=[4]),
    37: dict(name='Story scene (script #516)', confidence='low', context='script #516 (warp to area 0-96)', scripts=[516]),
    38: dict(name="Hercule Cafe / Kame House", confidence='high',
             context="Hercule Café 3-20..3-22, Hercule City 3-23, Roshi's Island (5-1), Kame House 5-2, 5-3"),
    39: dict(name='Sparring match', confidence='medium',
             context="script #215 (Kami's Lookout scene), #273 (World Tournament match vs Videl, 'You Lose!!' branch)",
             scripts=[215, 273]),
    42: dict(name='Giant Crater (after the blast)', confidence='high', context='Giant Crater 6-16 (Super Buu)'),
    44: dict(name='Boss battle', confidence='medium',
             context='scripted fights: #26 (West City, spawns Majin Buu type 28), #58 (warp to Yemma\'s Office), #313 '
                     '(World Tournament ring 7-6), #142, #305; World Tournament ring area 7-7', scripts=[26, 58, 142, 305, 313]),
    45: dict(name='Title screen', confidence='high', context='played by the boot code (0x080041CE, 0x0800540E)',
             evidence='PlaySong call sites 0x080041CE and 0x0800540E pass song table entry 45'),
    47: dict(name='Rocky Canyon', confidence='high', context='Rocky Canyon 8-70 (Majin Vegeta)'),
    49: dict(name='Story scene (scripts #103, #143)', confidence='low',
             context='script #103 (warp to West City 0-22), #143', scripts=[103, 143]),
    50: dict(name='World Tournament', confidence='high', context='World Tournament Stadium 7-4..7-6'),
    51: dict(name='West City (ruins)', confidence='high', context='areas 0-50, 0-51 (West City)'),
}

# trigger: what fires it in the game.  volume: the value the game passes (64 = full).
_H = 'breakpoint on PlaySfx 0x0803E378 while doing it in the emulator (work/audio/sfxtrace2.py, skillsfx.py)'
SFX = {
    3: dict(name='Ki charge (loop)', confidence='high',
            trigger='hold B with a beam / Big Bang / Masenko / Energy Sword / Final Flash / Flame Kamehameha: starts on the '
                    'press, stopped on release or when the beam fires', evidence=_H + ' - beam object 0x08020764, Big Bang 0x08037502; '
                                                                                     'lockstep (tools/battle/LOCKSTEP.md)'),
    20: dict(name='Energy blast A', confidence='high', trigger='Energy Blast fired (random pick of 20/21)',
             evidence=_H + ' - ki blast 0x08019846 rand(2)'),
    21: dict(name='Energy blast B', confidence='high', trigger='Energy Blast fired (random pick of 20/21)',
             evidence=_H + ' - ki blast 0x08019846 rand(2)'),
    23: dict(name='Explosion (Dragon Fist launch, Big Bang hit)', confidence='high',
             trigger='Dragon Fist Explosion launched (with stop 32); the Big Bang ball exploding',
             evidence='lockstep f41_skill5 (Dragon Fist launch: stop 32, play 23); static Big Bang 0x08037638'),
    24: dict(name='Dragon Fist Explosion', confidence='high', trigger='Dragon Fist Explosion activated (state enter)', evidence=_H + ' - 0x0801AB94'),
    25: dict(name='Dash (step, every 16 frames)', confidence='high', volume=32,
             trigger='dashing: the dash state 0x03002B3C plays it at volume 32 whenever its tick & 15 == 0 (16 frames '
                     'after the dash starts, then every 16 frames)',
             evidence='dash tick 0x08012FC4 (call site 0x08012FE0); lockstep f02_move'),
    31: dict(name='Instant Transmission', confidence='high', trigger='each teleport of Instant Transmission (one frame after the '
                                                                        'jump starts, with the EP cost)',
             evidence='IT state 0x0804E334 control (call site 0x0801BB76); emulator, forest save state'),
    30: dict(name='Hit impact (special attackers)', confidence='low',
             trigger='hit spark 0x0802A1D4 when the attacker graphic is 0x60 / 0xE5 / 0xDC', evidence='static 0x0802A204'),
    32: dict(name='Beam fire (loop)', confidence='high',
             trigger='beam / Final Flash / Flame Kamehameha firing, Big Bang ball launched, Dragon Fist dragon out, Energy '
                     'Sword swing; stopped when it ends',
             evidence=_H + ' - beam object 0x08020828, stop 0x08020790; lockstep'),
    33: dict(name='Beam end', confidence='high', trigger='a beam has retracted (3 px per frame after the release)',
             evidence=_H + ' - 0x080207A0; lockstep'),
    34: dict(name='Energy wave release', confidence='high',
             trigger='Burning Attack launch, Masenkoha release, Ultra Volleyball throw',
             evidence=_H + ' - 0x0802940A, 0x080214D8, 0x08038482; lockstep'),
    35: dict(name='Hit spark variant', confidence='low', trigger='hit spark object 0x0802A3A0', evidence='static 0x0802A3A0'),
    36: dict(name='Level up', confidence='high', trigger='level gained (bytecode script #3: push 36, native 0x1F)',
             evidence=_H + ' - killing an enemy with XP set one below the next level'),
    39: dict(name='Menu cursor', confidence='high', trigger='cursor moves in the pause / status menus', evidence=_H + ' - 0x08006032'),
    40: dict(name='Menu confirm', confidence='high', trigger='pause (START), menu select (A), cycle skill (L)',
             evidence=_H + ' - 0x0800535E pause, 0x0800B994 skill cycle, 0x0801E622 menu A'),
    41: dict(name='Low HP warning', confidence='high', volume=32,
             trigger='HUD: while 0 < HP <= maxHP / 5, whenever more than 60 frames passed since the last beep (every 61 '
                     'frames)', evidence=_H + ' - 0x0800BA6E, volume 32; lockstep f02_lowhp'),
    46: dict(name='Volleyball bounce', confidence='high', trigger='Ultra Volleyball Attack ball hit', evidence=_H + ' - 0x0803C8B6'),
    47: dict(name='Energy Punch', confidence='high', trigger='Energy Punch activated', evidence=_H + ' - 0x0801B278'),
    48: dict(name='Hit impact A', confidence='high',
             trigger='a hit lands: player melee, enemy melee, energy blast, beam ticks, Instant Transmission punch, Energy '
                     'Sword strike (random pick of 48/49 with the game RNG)',
             evidence=_H + ' - 0x08012AA4 player melee, 0x0802A258 hit spark, 0x080199E0 blast, 0x080209D6 beam'),
    49: dict(name='Hit impact B', confidence='high', trigger='same as 48 (random pick of 48/49)', evidence=_H),
    50: dict(name='Melee swing A', confidence='high',
             trigger='every punch (player A press or enemy punch), random pick of 50/51', evidence=_H + ' - 0x0800DF1A attack-type callback'),
    51: dict(name='Melee swing B', confidence='high', trigger='same as 50 (random pick of 50/51)', evidence=_H),
    54: dict(name='Super Saiyan transform / aura (loop)', confidence='high',
             trigger='the transformation state (Super Saiyan / SS3) loops it; the SS form objects play it on entering the '
                     'form and stop it on leaving (KO, revert); the transformation\'s end stops it again on the same frame',
             evidence=_H + ' - 0x0801C414, form object 0x0800E3B2 / 0x0800E3CE; lockstep f02_skill20 / ssdrain / ko'),
    59: dict(name='Dash into a wall', confidence='high', volume=64,
             trigger='a dash blocked by a wall: the bump (hurt animation, pushed back 0.75 px per frame, screen shake)',
             evidence='dash control 0x08012F5E; lockstep f02_bump'),
    62: dict(name='Hit impact (special attackers 2)', confidence='low',
             trigger='hit spark 0x0802A1D4 when the attacker graphic is 0xEE / 0xF4 / 0xED', evidence='static 0x0802A236'),
    63: dict(name='Enemy loop sound', confidence='low', trigger='looping enemy effect, stopped per instance (0x0803E32C)',
             evidence='static 0x08023FF4, 0x0802E224'),
}
# the native call sites without a breakpoint trace, named by the code around them (enemy AI nodes / projectile classes
# of data/buu/enemy_ai.json / bosses.json; work/ls/nodenames.py)
for _i, (_n, _c, _s, _t) in {
        11: ('Babidi gesture', 'low', '0x0802B078', 'in the code after the BabidiGesture node 0x0802AEAA (Babidi\'s magic)'),
        19: ('Heavy strike impact', 'low', '0x0802FC20', 'in the code after the HeavyStrike / IfPartnerHolding nodes (0x0802F918)'),
        28: ('Shell / grenade launch', 'medium', '0x0801C96A, 0x08026582, 0x08026802',
             'tank shell constructor 0x0801C900, the flame-thrower node at 0x080264EA, SoldierThrow 0x08026739'),
        38: ('Mechanoid / tank sound', 'low', '0x08031D68, 0x080337AE', 'Mechanoid family nodes (MechTransform 0x080315A4) and the '
                                                                          'tank line-of-fire node 0x080332E0'),
        44: ('Sound 44', 'low', '0x08017EF0', 'native code 0x08017EC2 (AI core)'),
        45: ('Turret sound', 'low', '0x0802BC8C', 'in the code after the turret nodes (TurretClose 0x0802B8B4)'),
        52: ('Sound 52', 'low', '0x0801CF2A, 0x0803D29A', 'native code 0x0801CDC0 and 0x0803D110 (Janemba parts)'),
        61: ('Ninja star throw', 'medium', '0x0802C128', 'NinjaStarThrow controller 0x0802C08A (spawns the ninja star)'),
}.items():
    SFX[_i] = dict(name=_n, confidence=_c, trigger=_t, evidence=f'static call site {_s}')
for _i, _s in {2: '#338', 4: '#160', 6: '#25, #460, #478', 7: '#460, #479', 8: '#483, #484', 15: '#57, #58, #518',
               16: '#324, #327, #330 ...', 17: '#9, #412, #415 ...', 22: '#131, #143, #169', 26: '#501, #502', 29: '#4, #338',
               37: '#102, #103, #342 ...', 42: '#38, #66, #91 ...', 53: '#412..#422, #483', 55: '#103, #330', 57: '#330, #415',
               }.items():
    SFX.setdefault(_i, dict(name=f'Cutscene sound {_i:02d}', confidence='medium', trigger=f'story scripts {_s}',
                            evidence='bytecode scan: push id; native 0x1F (PlaySfx)'))

# battle simulator: song per arena (= the area's own music, area record +0x24). Boss fights keep the area music: only
# five story scripts play the battle theme 44 (#26 Majin Buu's attack on West City, #58, #142, #305, #313 the World
# Tournament ring) plus the ring area 7-7 itself, so the simulator uses 44 only for the enemy types of those fights.
ARENA_MUSIC = {'crater': 1, 'volcano': 19, 'hfil': 18, 'wasteland': 27, 'demon': 1, 'canyon': 47, 'yakon': 1,
               'crater2': 42, 'htc': 21, 'stadium': 50, 'forest': 28}
BOSS_MUSIC = 44
# enemy types whose story fight plays BOSS_MUSIC: Majin Buu at West City (script #26), Spopovich / Yamu (World
# Tournament ring, script #313 / area 7-7)
BOSS_MUSIC_TYPES = [0x1C, 0x6F, 0x93]

# battle events -> SFX ids (pick one at random when several), with the game's volume
EVENTS = {
    'melee_swing': {'sfx': [50, 51]}, 'hit': {'sfx': [48, 49]}, 'ki_blast': {'sfx': [20, 21]},
    'charge': {'sfx': [3], 'loop': True}, 'beam': {'sfx': [32], 'loop': True}, 'beam_end': {'sfx': [33]},
    'wave': {'sfx': [34]}, 'big_bang': {'sfx': [23]}, 'dragon_fist': {'sfx': [24]}, 'energy_punch': {'sfx': [47]},
    'volleyball': {'sfx': [46]}, 'transform': {'sfx': [54]}, 'dash': {'sfx': [25], 'volume': 32}, 'bump': {'sfx': [59]},
    'level_up': {'sfx': [36]},
    'low_hp': {'sfx': [41], 'volume': 32}, 'menu': {'sfx': [40]}, 'cursor': {'sfx': [39]},
}
