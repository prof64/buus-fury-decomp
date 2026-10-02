"""script_nat_b_make.py -- write engine/annotations/script_natives_b.json (script natives 0x58..0xA7).

Curated from: handler disassembly (work/buu/script/natives_asm.txt, nat_b_vtables.txt), static stack-effect
analysis (script_nat_b_stack.py), usage census over the 367 real scripts of the script table
(script_nat_b_uses.py), and emulator runs through the game's own runner (script_nat_b_probe.py / _film.py /
_cam.py / _actors.py, harness script_nat_b_lib.py, state buu_snakeway_start.ss).

Shared semantics referenced below:
* ACTOR id (0x080001C6): 0 = player object [0x03001C34]; 9 = [0x03001BAC]; 1..8 and >=10 = sprite id (resolved via
  0x08000158: ids < 10 = party character -> its current form sprite; >= 10 = sprite table 0x086B6BD8 entry),
  then the first entity answering message 5 with that sprite definition; negative -n = the entity answering
  message 7 with key n-1 (map placement number). Unresolved -> NULL.
* PROP id: entity answering message 8 with that key (0x08011C70(registry 0x03001C4C, 8, id)) -- scripted map
  props (e.g. the stamps/papers on Yemma's desk). Track = prop+0xC. Position ints at +0x24/+0x28, rect +0x2C,
  visible flag +0x1C.
* ENQUEUE (0x0801936A(director 0x03002BAC, cmd, track)): track = actor+0x18C (actor command queue), prop+0xC, or
  the global track [0x03002BD8] for camera commands. NULL track -> the command is destroyed, nothing happens.
  If the director is not in batch mode (native 0x38 begin_batch sets director[0]=1; native 0x3A end_batch runs
  the wait mode and clears it), the native immediately runs the sequence mode 0x0804C0C4 (nested main loop)
  until all queues are empty -> blocks >= 1 frame. In batch mode commands only queue; tracks run in parallel,
  each track runs its commands in order. Command protocol: vt[1] start(cmd, track_holder), vt[2] per-frame,
  vt[3] done?(cmd) (returns 1 when finished), vt[0] destroy.
* MODAL: the native pushes a game mode and runs a nested main loop (0x0803D8A0) until the mode exits; the
  script is suspended for that many frames. Returns the mode's exit code where noted.
* Speeds in 1/100 px per frame: frames = (isqrt((dx^2+dy^2)<<16)*100/speed >> 8) + 1 (0x080192CE).
"""
import json
from common import *

ROOTD = ROOT
uses = json.loads((WORK / 'script' / 'nat_b_uses.json').read_text())
NAT = 0x08055098

E = 'enqueue'
Q = 'query'
I = 'immediate'
M = 'modal'
C = 'conditional'

# id: (pops, pushes, args, name, category, kind, blocking, engine_fn, semantics, status, evidence)
T = {
 0x58: (2, 0, ['actor', 'item'], 'show_item_get', 'item', E, C,
        '0x0801936A enqueue cmd vt 0x0804C83C on actor track',
        'Item-get presentation on ACTOR without changing the inventory: start plays SFX 45 (table 0x083192A8 '
        'entry 0x87*4/12), puts the actor in its hold-up pose (actor vt+0x14 / 0x0800F53C) and spawns the item '
        'icon above it (item >= 0: supply table 0x083BD2F8 icon; item < 0: equipment -item, table 0x086F82EC); '
        'done after 60 frames, then the pose is released (0x0800F558 / vt+0x28).',
        'INFERRED', 'same command class as native 0x80 (VERIFIED there); unused in table scripts'),
 0x59: (7, 0, ['actor', 'anim', 'frame_frames', 'loops', 'wait', 'loop_start', 'last_frame'], 'play_anim_frames',
        'actor', E, C, '0x0801936A enqueue cmd vt 0x0804E268; controller 0x0804DE58 via 0x0800ECDA/0x0800ED18',
        'Play actor animation ANIM (state id = table 0x08072670[anim], identity table) frame by frame: each cel '
        'lasts FRAME_FRAMES frames, cels 0..LAST_FRAME; after the last cel restart at LOOP_START; LOOPS > 0 = '
        'number of passes (0 = forever). If WAIT != 0 the command completes when the passes are done (then the '
        'actor returns to idle, 0x0800EE78); if WAIT == 0 the command completes immediately and the animation '
        'keeps running.',
        'VERIFIED', 'controller update 0x0802CB05 read; emulator: (161,0,5,0,0,2,4) returned at once, (161,0,2,1,1,0,4) '
        'on an NPC never completed (anim has no cel 4); used e.g. script 329 on spawned sprite 89'),
 0x5a: (2, 0, ['actor', 'emote'], 'emote_wait', 'actor', E, C,
        '0x0801936A enqueue cmd vt 0x0804C850 -> 0x08016304 (balloon object, graphics table 0x086B8128[emote])',
        'Show emote balloon EMOTE (sweat drop, anger mark, ...) above ACTOR and complete when the balloon '
        'animation ends (~23 frames).',
        'VERIFIED', 'emulator: icon above Goku, call took 26 frames (film_emote.png)'),
 0x5b: (2, 0, ['actor', 'emote'], 'emote', 'actor', E, C,
        '0x0801936A enqueue cmd vt 0x0804DD44 -> 0x08016304',
        'Same balloon as emote_wait but the command completes immediately (balloon plays on its own).',
        'VERIFIED', 'emulator: returned after 1 frame + wait'),
 0x5c: (3, 0, ['actor', 'dir', 'frames'], 'fire_beam', 'actor', E, C,
        '0x0801936A enqueue cmd vt 0x0804DD58 -> effect 0x080206B8 (offsets table 0x0877B254[dir], 12 bytes)',
        'ACTOR takes its firing pose (state 0x38) facing DIR and emits an energy beam effect from the table offset '
        'for that direction; after 24 frames the beam is extended (0x080207D2); command completes after FRAMES '
        'frames, beam removed (0x0802082A), actor back to idle.',
        'VERIFIED', 'emulator film_f5c.png: Goku fires a beam downwards for 40 frames; unused in table scripts'),
 0x5d: (3, 0, ['actor', 'x', 'y'], 'set_pos', 'actor', E, C,
        '0x0801936A enqueue cmd vt 0x0804B940',
        'Teleport ACTOR to (X, Y) px (actor+0x1A0/+0x1A4 = v<<8, around actor vt+0x3C/+0x40 refresh); completes '
        'at once.',
        'VERIFIED', 'emulator: set_pos(161,120,124) then actor_x/actor_y read back 120/124'),
 0x5e: (3, 0, ['actor', 'dir', 'frames'], 'fire_blast', 'actor', E, C,
        '0x0801936A enqueue cmd vt 0x0804C864 -> effect 0x080206B8 (offsets table 0x0877B2A8[dir])',
        'Like fire_beam but with the wide flame/ki-wave effect (graphic 0x08725038).',
        'VERIFIED', 'emulator film_f5e.png: orange blast below Goku; unused in table scripts'),
 0x5f: (4, 0, ['actor', 'target', 'dx', 'dy'], 'walk_to_actor', 'actor', E, C,
        '0x0801936A enqueue cmd built by 0x08015AF0(0, target_obj, &{dx,dy})',
        'Walk ACTOR to the position of actor TARGET offset by (DX, DY) px at 1 px/frame; completes on arrival.',
        'VERIFIED', 'emulator: walk_to_actor(0,161,16,0) moved the player from (144,152) to (98,192) in 63 frames'),
 0x60: (3, 0, ['actor', 'x', 'y'], 'walk_to', 'actor', E, C,
        '0x0801936A enqueue cmd built by 0x0805082C(0, &{x,y}, 1, 100)',
        'Walk ACTOR to (X, Y) px at speed 100 (1 px/frame); completes on arrival.',
        'VERIFIED', 'emulator: walk_to(0,200,180): player reached (200,180) in 65 frames'),
 0x61: (4, 0, ['actor', 'x', 'y', 'speed'], 'walk_to_speed', 'actor', E, C,
        '0x0801936A enqueue cmd built by 0x0805082C(0, &{x,y}, 1, speed)',
        'Walk ACTOR to (X, Y) at SPEED/100 px per frame; completes on arrival.',
        'VERIFIED', 'emulator: same path at speed 100 = 65 frames, 300 = 23 frames'),
 0x62: (2, 0, ['prop', 'vanish'], 'prop_poof', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804E0FC on prop track',
        'Play the 8-frame-per-step dust/poof frames (table 0x087581FC, LZ-inflated into the prop OBJ VRAM) on PROP: '
        '5 steps normally; with VANISH != 0, 8 steps and the prop is hidden at the end (+0x1C = 0).',
        'VERIFIED', 'emulator: prop_poof(1,1) on Yemma\'s desk took 86 frames incl. wait; unused in table scripts'),
 0x63: (1, 0, ['prop'], 'prop_hide', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804DEB4 (+8 = 0) on prop track',
        'Hide map prop PROP (prop+0x1C = 0); completes at once.',
        'VERIFIED', 'emulator: desk props 0..2 in Yemma\'s office vanish (props.png)'),
 0x64: (1, 0, ['prop'], 'prop_show', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804DEB4 (+8 = 1) on prop track',
        'Show map prop PROP (prop+0x1C = 1); completes at once.',
        'VERIFIED', 'emulator: hidden desk props reappear (props.png)'),
 0x65: (3, 0, ['prop', 'x', 'y'], 'prop_effect_at', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804C43C on prop track -> 0x0801952C(0, &{x,y}, 0x37)',
        'Spawn effect object kind 0x37 at (X, Y) (after reserving 0x400 bytes in pool 0x0874EA44), sequenced on '
        'PROP\'s track; completes at once.',
        'INFERRED', 'handler + start method read; unused in table scripts'),
 0x66: (4, 0, ['prop', 'x', 'y', 'speed'], 'prop_move_to', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804B92C on prop track; step math 0x080192CE',
        'Move PROP linearly to (X, Y) px at SPEED/100 px per frame (frame count from 0x080192CE); completes on '
        'arrival.',
        'VERIFIED', 'emulator: props 0..2 in Yemma\'s office moved, each call blocked ~41 frames; script 57 uses '
        'three in a batch'),
 0x67: (3, 0, ['prop', 'x', 'y'], 'prop_jump_to', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804DD08 on prop track',
        'Move PROP to (X, Y) along a jump arc in 44 frames (vx = dx*0x5D3>>8, vy0 = dy*0x5D3-0xBA60>>8 - 0x200, '
        'gravity +0x20/frame, 1/256 px units); completes on landing.',
        'VERIFIED', 'emulator: prop 0 hopped off the desk, call 48 frames (props.png); start method 0x0802BDF5'),
 0x68: (2, 0, ['prop', 'frames'], 'prop_wait', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804AB58 (countdown 0x080159F5) on prop track',
        'Wait FRAMES frames on PROP\'s track (sequencing inside a batch).',
        'INFERRED', 'update method read; used between prop_move_to batches (script 57)'),
 0x69: (3, 0, ['prop', 'x', 'y'], 'prop_set_pos', 'map', E, C,
        '0x0801936A enqueue cmd vt 0x0804CD60 on prop track',
        'Teleport PROP to (X, Y) (moves +0x24/+0x28 and its rect +0x2C by the delta); completes at once.',
        'INFERRED', 'start method 0x0802CAB9 read'),
 0x6a: (2, 0, ['actor', 'frames'], 'camera_follow_wait', 'camera', E, C,
        '0x0801936A enqueue cmd vt 0x0804DCCC on the global track [0x03002BD8]',
        'Make the camera follow ACTOR (same follow objects as native 0x33: 0x0804E324 or 0x0804D300 via '
        '0x08021094), then wait FRAMES frames.',
        'INFERRED', 'start method 0x0802111D read; unused in table scripts'),
 0x6b: (3, 0, ['x', 'y', 'speed'], 'camera_pan_to', 'camera', E, C,
        '0x0801936A enqueue cmd built by 0x08021174(0, &{x,y}, speed) on the global track',
        'Pan the camera so its top-left corner moves to (X, Y) px at SPEED/100 px per frame (frame count fixed '
        'at start by 0x080192CE); the result is clamped to the map bounds; completes after the computed frames. '
        'Camera following is stopped.',
        'VERIFIED', 'emulator: camera_pan_to(144,300,500) moved camera 0x030019A4 from (32,72) to (144,256 '
        'clamped) in 53 frames; speed 60 did not finish within 400 frames (254 px -> 423 frames)'),
 0x6c: (1, 0, ['frames'], 'camera_release_wait', 'camera', E, C,
        '0x0801936A enqueue cmd vt 0x0804B2A0 on the global track; 0x080210B6',
        'Stop camera following (0x080210B6, same as native 0x34), then wait FRAMES frames.',
        'VERIFIED', 'emulator: camera_release_wait(30) blocked 32 frames'),
 0x6d: (1, 0, ['v'], 'camera_set_param8', 'camera', E, C,
        '0x0801936A enqueue cmd vt 0x0804C7EC on the global track; soft-float 0x08054ADC.. on camera 0x030019A4',
        'Stop following; camera+8 = V<<10 and derive camera+0x10 from it with float math (constants 0x3BB7E762, '
        '0x4222F9DB, 0x44800000), then refresh (0x0802ED98). Purpose unknown (third camera coordinate / depth).',
        'INFERRED', 'start method 0x08036D85 read; emulator: no visible change on world 6 map 16; unused'),
 0x6e: (1, 0, ['v'], 'camera_set_param12', 'camera', E, C,
        '0x0801936A enqueue cmd vt 0x0804E0D4 on the global track',
        'Stop following; camera+0xC = V<<10; refresh (0x0802ED98).',
        'INFERRED', 'start method 0x08036DD7 read; unused in table scripts'),
 0x6f: (2, 0, ['x', 'y'], 'camera_set', 'camera', E, C,
        '0x0801936A enqueue cmd vt 0x0804D480 on the global track; camera 0x030019A4',
        'Stop following and put the camera top-left at (X, Y) px (camera+0/+4 = v<<10; through 0x080094F0 when '
        'the camera mode flag camera+0x90 is clear); completes at once.',
        'VERIFIED', 'emulator: camera_set(10,20) -> camera ints (10,20); camera_set(120,32) on world 6 map 16'),
 0x70: (3, 0, ['x', 'y', 'z'], 'camera_set3', 'camera', E, C,
        '0x0801936A enqueue cmd vt 0x0804D494 on the global track',
        'Stop following; camera+0/+4/+8 = X/Y/Z << 10; refresh.',
        'VERIFIED', 'emulator: camera words became 0x4B000/0x32000/0x19000 for (300,200,100); unused in table scripts'),
 0x71: (0, 0, [], 'save_field_on_next_scene', 'map', I, False,
        'writes 0x03001004+8 = 1',
        'Arm a flag so that the next load_scene / load_scene_* (0x72..0x74) first snapshots the current field '
        '(0x08009196: world descriptor -> 0x03001968, camera, map objects, entities) so that return_to_field (0x75) '
        'can restore it.',
        'VERIFIED', 'emulator: save_field_on_next_scene; load_scene_fade(0,9,...); return_to_field restored Snake '
        'Way with the player at (144,152) (probe_rt75.png)'),
 0x72: (5, 0, ['world', 'map', 'variant', 'x', 'y'], 'load_scene_fade', 'map', M, True,
        'fade out 0x08050714(0,64)+0x0803D8A0; 0x08009030(world_obj 0x0300197C, world, map, variant, &{x,y}, 1, 1); '
        'fade-in mode 0x0804C588 (16 frames)',
        'Fade out (64 frames), load map WORLD/MAP as a cutscene scene (no player object), layout VARIANT '
        '(0x080089A0), camera top-left at (X, Y), running the map\'s load scripts (map desc +0x34, +0x30) and '
        'spawning its objects; fade back in over 16 frames. Saves the field first if save_field_on_next_scene '
        'was armed.',
        'VERIFIED', 'emulator: (0,9,0,170,138) took 104 frames, player pointer NULL afterwards'),
 0x73: (5, 0, ['world', 'map', 'variant', 'x', 'y'], 'load_scene', 'map', I, False,
        '0x08009030(world_obj, world, map, variant, &{x,y}, 1, 0)',
        'Load map WORLD/MAP as a scene immediately (no fades; scripts wrap it in fade_out/fade_in), same as '
        'load_scene_fade otherwise (last flag 0).',
        'VERIFIED', 'emulator: (0,9,0,170,138) -> new map, no player; script #1: fade_out(16); load_scene(0,2,0,0,0); '
        'fade_in(16)'),
 0x74: (5, 0, ['world', 'map', 'variant', 'x', 'y'], 'load_scene_fade_noobj', 'map', M, True,
        'like 0x72 but 0x08009030(..., 0, 1)',
        'As load_scene_fade but with the first flag 0: the loop that spawns the map\'s conditional objects '
        '(0x08004784 condition test per map object) is skipped.',
        'INFERRED', 'handler diff vs 0x72 (flag at sp+4 = 0); unused in table scripts'),
 0x75: (0, 0, [], 'return_to_field', 'map', M, True,
        'fade out 16; 0x080091B8(world_obj) restore snapshot + run map script +0x34; fade-in mode 16',
        'Fade out (16), restore the field saved by save_field_on_next_scene (world descriptor, camera, objects, '
        'entities; re-runs the map\'s +0x34 script), fade in (16).',
        'VERIFIED', 'emulator roundtrip after 0x71 + 0x72 (probe_rt75.png); 0x080091B8 is the inverse of 0x08009196'),
 0x76: (5, 0, ['world', 'map', 'dir', 'x', 'y'], 'warp_fade', 'map', M, True,
        '0x03002A58 = 1; player vt+0x28(0); fade out 64; 0x08008D44(world_obj, world, map, &{x,y}, dir, 1); '
        'fade-in mode 0x080495B4 (4)',
        'Fade out, move the PLAYER to map WORLD/MAP at (X, Y) facing DIR (camera centred: x-112, y-76), fade in; '
        'returns when visible again.',
        'VERIFIED', 'emulator: (0,9,0,170,138) took 152 frames, player at (170,138) on the new map'),
 0x77: (5, 0, ['world', 'map', 'dir', 'x', 'y'], 'warp', 'map', I, False,
        '0x03002A58 = 1; player vt+0x28(0); 0x08008D44(world_obj, world, map, &{x,y}, dir, 0)',
        'Move the PLAYER to map WORLD/MAP at (X, Y) facing DIR immediately (no fades; scripts wrap it).',
        'VERIFIED', 'emulator: warp(0,9,0,170,138) -> player (170,138); warp(1,3,1,232,360) -> Yemma\'s Office'),
 0x78: (1, 0, ['frames'], 'fade_out', 'screen', M, True, '0x08050714(0, frames) + 0x0803D8A0',
        'Fade the screen to black over FRAMES frames.', 'VERIFIED',
        'emulator: fade_out(16) blocked 17 frames, black screen (probe_0x78_16.png)'),
 0x79: (1, 0, ['frames'], 'fade_in', 'screen', M, True, 'mode vt 0x0804C588 (+8 = frames)',
        'Fade back in from black over FRAMES frames.', 'VERIFIED', 'emulator: fade_out(16)+fade_in(16) = 33 frames'),
 0x7a: (1, 0, ['frames'], 'fade_out_white', 'screen', M, True, 'mode vt 0x0804BBD4 built by 0x080176AC (0x40C bytes)',
        'Fade the screen to white over FRAMES frames.', 'VERIFIED', 'emulator: probe_0x7a_16.png (white)'),
 0x7b: (1, 0, ['frames'], 'fade_in_white', 'screen', M, True, 'mode vt 0x0804C5B8',
        'Fade back in from white over FRAMES frames.', 'VERIFIED', 'emulator: probe_0x7a_16__0x7b_16.png'),
 0x7c: (1, 0, ['frames'], 'mosaic_out', 'screen', M, True, 'mode vt 0x0804A5B4 (MOSAIC on BG0..3, palettes cleared)',
        'Pixelate (mosaic) the screen over FRAMES frames, ending black (palette RAM zeroed).', 'VERIFIED',
        'emulator: probe_0x7c_16.png; BGxCNT mosaic bit set'),
 0x7d: (1, 0, ['frames'], 'mosaic_in', 'screen', M, True, 'mode vt 0x0804A070',
        'Reverse of mosaic_out over FRAMES frames.', 'VERIFIED', 'emulator: probe_0x7c_16__0x7d_16.png'),
 0x7e: (1, 0, ['frames'], 'flash_white', 'screen', M, True, 'mode vt 0x0804A9D8 (frames clamped to >= 2)',
        'White flash: fade to white and back, ~2*FRAMES+1 frames in total.', 'VERIFIED',
        'emulator: flash_white(10) blocked 21 frames (probe_0x7e_10.png)'),
 0x7f: (1, 1, ['item'], 'item_count', 'item', Q, False, 'reads u8 GameData(0x03001068)+2+item',
        'Number of supply item ITEM in the inventory.', 'VERIFIED', 'emulator: 0 -> give_item -> 1 -> 11 -> 10 -> 7'),
 0x80: (1, 0, ['item'], 'give_item', 'item', E, C,
        'GameData+2+item += 1 (saturates at 255); enqueue cmd vt 0x0804C83C on the player track',
        'Add one supply item ITEM and play the item-get presentation on the player (60 frames, SFX 45).',
        'VERIFIED', 'emulator: count 0 -> 1, call blocked 61 frames'),
 0x81: (2, 0, ['item', 'n'], 'add_item', 'item', I, False, 'GameData+2+item += n (saturates at 255)',
        'Add N of supply item ITEM silently.', 'VERIFIED', 'emulator: 1 -> 11'),
 0x82: (1, 0, ['item'], 'take_item', 'item', I, False, 'GameData+2+item -= 1 (u8 wrap, no check)',
        'Remove one supply item ITEM (no underflow check).', 'VERIFIED', 'emulator: 11 -> 10'),
 0x83: (2, 0, ['item', 'n'], 'take_items', 'item', I, False, 'GameData+2+item -= n (u8 wrap)',
        'Remove N of supply item ITEM (no underflow check).', 'VERIFIED', 'emulator: 10 -> 7'),
 0x84: (1, 0, ['size'], 'drop_zenie', 'item', I, False, '0x08018346(&loot_pos 0x03001010, size)',
        'Drop a zenie coin of SIZE (0..3 = 5/25/50/100 Z, coin defs 0x086FC250) at the loot position '
        '0x03001010 (set by the dying enemy).', 'VERIFIED', 'loot model reproduction (notes §6, script #500)'),
 0x85: (1, 0, ['item'], 'drop_item', 'item', I, False,
        '0x0800D30E(0, item, &loot_pos, ...) added to registry 0x03001C4C',
        'Drop supply item ITEM (table 0x083BD2F8) at the loot position, only if enough free memory (0x080116FC >= '
        '0x1000) and object slots remain.', 'VERIFIED', 'loot model reproduction (script #499)'),
 0x86: (1, 0, ['item'], 'drop_equipment', 'item', I, False, 'as 0x85 with -item',
        'Drop equipment ITEM (table 0x086F82EC) at the loot position (same capacity checks).',
        'VERIFIED', 'loot model reproduction (script #496)'),
 0x87: (3, 0, ['x', 'y', 'size'], 'drop_zenie_at', 'item', I, False, '0x08018346(&{x,y}, size, zero velocity 0x08054FF0)',
        'Drop a zenie coin of SIZE at (X, Y).', 'INFERRED', 'handler read; unused in table scripts'),
 0x88: (1, 0, ['frames'], 'wait', 'system', M, True, 'mode vt 0x08049978 (+4 = frames)',
        'Suspend the script for FRAMES frames (the world keeps running).', 'VERIFIED',
        'emulator: wait(30) blocked 31 frames'),
 0x89: (0, 0, [], 'wait_button', 'system', M, True, 'mode vt 0x0804CF64',
        'Suspend until the A button is pressed.', 'VERIFIED',
        'emulator: returned 1 frame after A was pressed; START did not end it'),
 0x8a: (1, 0, ['index'], 'spawn_map_object', 'map', I, False,
        '0x08009172(world_obj, index): current map desc+0x10 entry index -> factory -> object list [0x080716A0+0x10]',
        'Spawn object INDEX of the current map\'s object list unconditionally (ignores its story-flag condition).',
        'INFERRED', 'handler read; cf. 0x08008AA0 which spawns only objects whose condition holds'),
 0x8b: (0, 0, [], 'refresh_map_objects', 'map', I, False,
        '0x08008AA0(world_obj, 0): clear registry (0x08012900), reload layout (0x08009670), respawn map objects '
        'whose condition 0x08004784 holds; 0x03002A58 = 1 if [0x03002A64]+0xCC',
        'Re-evaluate the current map\'s conditional objects (after story flags changed, e.g. a chest opened or '
        'an NPC left).', 'INFERRED', 'used right after give_item / flag changes in 42 scripts'),
 0x8c: (1, 1, ['flag'], 'save_menu', 'menu', M, True,
        'quick fade 0x0805058C; mode vt 0x08049504 (0x304 bytes, base 0x08005460(obj,7,0xD,0)); result pushed; '
        'restore + fade in 0x08050568',
        'Open the Save Game screen (FLAG stored at mode+0x2F8, meaning unknown); pushes exit code - 1. If there is '
        'no player object nothing happens and NOTHING is pushed (stack imbalance in the original).',
        'VERIFIED', 'emulator: Save Game screen (probe_m8c.png); scripts 11/12 branch on the result'),
 0x8d: (0, 0, [], 'character_menu', 'menu', M, True,
        'mode 0x0802E83C(0, player); on result 2 the player sprite is rebuilt for the new character',
        'Open the Switch Character screen; if the character changed, refresh the player sprite/controller.',
        'VERIFIED', 'emulator: Switch Character screen (probe_m8d.png); unused in table scripts'),
 0x8e: (0, 0, [], 'return_to_title', 'system', I, False,
        '0x03002A58 = 1; clear registry 0x08011B04; 0x08012628; stop music 0x0803E2EC; pop all modes 0x0803D8F2; '
        'replace mode with vt 0x0804A404 (plays song 45 = title music)',
        'Abort the game and switch to the title-screen mode (song 45). The script returns immediately (its '
        'mode stack is gone).', 'INFERRED', 'emulator: screen went black, call returned after 1 frame; song 45 is '
        'the title theme (data/buu/audio.json)'),
 0x8f: (1, 0, ['actor'], 'show_boss_bar', 'battle', I, False,
        '0x080195A8(0, actor_obj, 0, 100) -> HP gauge object vt 0x0804CE28 (OBJ pool 0x087196E0)',
        'Show the boss HP gauge (slides in at the top right) bound to ACTOR.', 'VERIFIED',
        'emulator: red gauge slid in (film_hpbar.png); script 228 calls it before a fight'),
 0x90: (3, 0, ['actor', 'a', 'b'], 'show_boss_bar_ex', 'battle', I, False, '0x080195A8(0, actor_obj, a, b)',
        'show_boss_bar with explicit gauge parameters A (default 0) and B (default 100).', 'INFERRED',
        'same constructor as 0x8F; unused in table scripts'),
 0x91: (0, 0, [], 'hide_boss_bar', 'battle', I, False,
        '0x08011C70(registry, 0x13, 0): message 0x13 to entities; the gauge class (0x08019721) answers by '
        'detaching from its actor',
        'Detach/hide the boss HP gauge (first gauge that answers message 0x13).', 'INFERRED',
        'message handler 0x08019721 (msg 0x13: +0x10 = 0, +0x17 = 0xFF); used at the start of 36 cutscenes'),
 0x92: (1, 1, ['item'], 'equipment_count', 'item', Q, False, '0x0800441E(GameData, item): list at GameData+0xE4',
        'Number of copies of equipment ITEM owned (entries of the equipment list whose low 28 bits == ITEM).',
        'VERIFIED', 'emulator: 0 -> give_equipment -> 1 -> take_equipment -> 0'),
 0x93: (1, 0, ['item'], 'give_equipment', 'item', E, C,
        '0x08004448(GameData, item) (append, max 250); item-get cmd vt 0x0804C83C on the player track with -item',
        'Add equipment ITEM and play the item-get presentation on the player.', 'VERIFIED',
        'emulator: count 0 -> 1, call blocked ~60 frames'),
 0x94: (1, 0, ['item'], 'take_equipment', 'item', I, False, '0x08004468(GameData, item)',
        'Remove one copy of equipment ITEM from the list.', 'VERIFIED', 'emulator: 1 -> 0'),
 0x95: (1, 0, ['char'], 'set_character', 'party', I, False,
        'GameData[0] = char; Holy Water flag GameData+0x4DC = 0; 0x08010692(player, rec) ; 0x0801712C/0x0801711C '
        'release/load form graphics',
        'Make party character CHAR (0 Goku .. 8) the active character, cancel the Holy Water bonus and rebuild '
        'the player object for the new character.', 'VERIFIED',
        'emulator: set_character(1) -> GameData[0]=1, player redrawn as the new character (switch95.png)'),
 0x96: (1, 1, ['char'], 'in_party', 'party', Q, False, 'char record (0x03001560+0x68*char) byte0 bit0',
        'Whether character CHAR has joined the party (record flag bit 0).', 'VERIFIED',
        'emulator: in_party(0)=1, in_party(1)=0 -> join -> 1'),
 0x97: (1, 0, ['char'], 'join_party', 'party', I, False, 'record byte0 |= 1',
        'Character CHAR joins the party.', 'VERIFIED', 'emulator'),
 0x98: (1, 0, ['char'], 'leave_party', 'party', I, False, 'record byte0 &= ~1',
        'Character CHAR leaves the party.', 'VERIFIED', 'emulator'),
 0x99: (1, 0, ['char'], 'set_char_flag2', 'party', I, False, 'record byte0 |= 2',
        'Set record flag bit 1 of CHAR (meaning not identified; already set for character 1 in the snakeway save).', 'VERIFIED',
        'emulator: byte0 0x02 stays 0x02, clear -> 0x00 (effect of the bit unknown)'),
 0x9a: (1, 0, ['char'], 'clear_char_flag2', 'party', I, False, 'record byte0 &= ~2',
        'Clear record flag bit 1 of CHAR.', 'VERIFIED', 'emulator: 0x02 -> 0x00'),
 0x9b: (0, 1, [], 'is_super_saiyan', 'party', Q, False,
        'form table 0x086B82EC[active rec+0x14].flags & 3',
        'Whether the active character is in a Super Saiyan or SS3 form (form flag bits 0/1).', 'VERIFIED',
        'emulator: 0 in base form; used to pick transformed poses in cutscenes'),
 0x9c: (1, 1, ['char'], 'char_sprite', 'party', Q, False, 'form table[rec(char)+0x14].u16 +0xE (gfx id)',
        'Sprite id of character CHAR\'s current form (e.g. 117 = Goku base).', 'VERIFIED',
        'emulator: char_sprite(0)=117, after set_char_form(0,129) = 129'),
 0x9d: (2, 0, ['char', 'sprite'], 'set_char_form', 'party', I, False,
        '0x08013DD6(rec, sprite); if char is active: 0x0801712C, player 0x0800EDF8(sprite def 0x08000158(sprite)), '
        'vt+0x28, 0x08010DAA, 0x0801711C',
        'Set character CHAR\'s form to the form whose sprite is SPRITE; if CHAR is active, rebuild the player '
        'sprite.', 'VERIFIED', 'emulator: form byte 2 -> 34 for sprite 129'),
 0x9e: (2, 0, ['x', 'y'], 'spawn_marker_a', 'system', I, False,
        '0x08050690(0, &{x,y}, gfx desc 0x08055000, 0) static OBJ object vt 0x0804BA50',
        'Spawn a static OBJ sprite object (64x64 graphic descriptor 0x08055000) at (X, Y).', 'INFERRED',
        'emulator: nothing visible (blank/debug graphic); unused in table scripts'),
 0x9f: (2, 0, ['x', 'y'], 'spawn_marker_b', 'system', I, False,
        '0x08050690(0, &{x,y}, gfx desc 0x0805500C, 0)',
        'As spawn_marker_a with graphic descriptor 0x0805500C.', 'INFERRED', 'emulator: nothing visible; unused'),
 0xa0: (4, 0, ['x', 'y', 'w', 'h'], 'spawn_box', 'system', I, False, '0x080279E0(0, &{x,y,w,h})',
        'Spawn a box object at (X, Y) with size (W, H) (drawn as a solid dark-red 16x16 sprite; debug/blocker).',
        'INFERRED', 'emulator: red square appeared at (144,140) (film_fa0.png); unused in table scripts'),
 0xa1: (1, 1, ['actor'], 'actor_x', 'actor', Q, False, 'actor+0x1A0 >> 8',
        'X position (px) of ACTOR.', 'VERIFIED', 'emulator: actor_x(161)=82 / 120 after set_pos'),
 0xa2: (1, 1, ['actor'], 'actor_y', 'actor', Q, False, 'actor+0x1A4 >> 8',
        'Y position (px) of ACTOR.', 'VERIFIED', 'emulator'),
 0xa3: (1, 1, ['actor'], 'actor_dir', 'actor', Q, False, 'u8 actor+0x10',
        'Facing direction of ACTOR (0 down, 1 up, 2 left, 3 right).', 'VERIFIED', 'emulator: 3 for NPC 161, 2 for the player'),
 0xa4: (2, 0, ['char', 'skill'], 'learn_skill', 'party', I, False, '0x08013A90(rec, skill)',
        'Append SKILL to character CHAR\'s skill list (rec+0x0D.., count rec+0x0C) at level 0 (rec+0x2E+2*i = 0).',
        'VERIFIED', 'emulator: count 3 -> 4, skill 22 appended'),
 0xa5: (2, 0, ['char', 'skill'], 'forget_skill', 'party', I, False, '0x08013ABA(rec, skill)',
        'Remove SKILL from CHAR\'s list (shifting ids and level pairs down; reset the selected skill if out of range).',
        'VERIFIED', 'handler read; emulator no-op when absent'),
 0xa6: (3, 0, ['char', 'old', 'new'], 'replace_skill', 'party', I, False, '0x08013B12(rec, old, new)',
        'Replace skill id OLD with NEW in CHAR\'s list, keeping its slot and level.', 'VERIFIED',
        'emulator: 22 -> 9'),
 0xa7: (1, 1, ['level'], 'watch_this_game', 'menu', M, True, 'mode 0x08027DB0(0, level); result - 1 pushed',
        'Run the "Watch This!" button-sequence challenge (memorise and repeat button prompts) with difficulty '
        'LEVEL; pushes exit code - 1 (success flag).', 'VERIFIED',
        'emulator: "Watch This!" window with button prompts (probe_ma7.png); result meaning INFERRED'),
}

out = []
for n in range(0x58, 0xa8):
    p, q, args, name, cat, kind, blk, efn, sem, st, ev = T[n]
    u = uses[str(n)]
    out.append({'id': n, 'handler': f'0x{u32(NAT + 4 * n):08X}', 'pops': p, 'pushes': q, 'args': args,
                'name': name, 'category': cat, 'kind': kind, 'blocking': blk, 'engine_fn': efn,
                'semantics': sem, 'status': st, 'evidence': ev, 'uses': u['uses'],
                'scripts': u['nscripts']})
    assert len(args) == p, hex(n)
dst = ROOTD / 'engine' / 'annotations' / 'script_natives_b.json'
dst.write_text(json.dumps(out, indent=1) + '\n')
print(len(out), 'natives;', sum(1 for x in out if x['status'] == 'VERIFIED'), 'verified ->', dst)
