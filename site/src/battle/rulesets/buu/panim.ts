// Buu's Fury actor animation model (verified frame by frame against the emulator: tools/battle/LOCKSTEP.md).
//
// The game has no animation scripts. An actor shows the animation struct at obj+0x50 (def+0x50 + 4*raw, raw =
// sprites.json raw_index) at frame obj+0x11; its *state object* (obj+0x14) owns the timing: per frame the actor update
// (0x0800EF08) calls the state's tick (vtable +4), then the state's control (+8, the player's input handling) of
// whatever state is current after the tick. The ticks are a handful of shared functions:
//   once  0x0800DE6C  tick++; f = tick >> shift; f >= nframes -> end callback (vtable +0x24) else frame = f
//   loop  0x0800DEB0  tick++; f = tick >> shift; f >= nframes -> tick = frame = 0
//   idle  0x0800DFD4  2-frame anims: frame = (tick & 0xFF) > 0xF0 (blink); else loop with shift 3
//   bored 0x0800DF8A  frame = tick >> 5; past the end -> tick 64, frame 2 (loops frames 2..n-1)
//   hold  0x0804E4E1  nothing (frame kept)
// SetAnim(id) (player vfunc +0x28, 0x0800ECF0) = raw (table 0x08072670) + the state for the id (0x0807273C).

/** SetAnim id -> raw animation index (0x08072670 + 1) */
export const ANIM_RAW = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 19, 20, 21, 22, 23, 24, 11, 12]

/** the state singletons of 0x0807273C (id 0 = the form controller's idle state) */
export type GenState = 'idle' | 'walk' | 'dash' | 'once3' | 'punch' | 'hold0' | 'bored' | 'loop_c' | 'once4' | 'loop_b' | 'once_hold' | 'ss_up' | 'ss_revert'
export const ANIM_STATE: GenState[] = [
  'idle', 'walk', 'dash', 'once3', 'punch', 'punch', 'punch', 'hold0', 'bored', 'once3', 'loop_c', 'loop_c', 'once4', 'loop_c', 'loop_c',
  'loop_b', 'once_hold', 'once4', 'once4', 'once4', 'once4', 'once4', 'once4', 'ss_up', 'ss_revert',
]

/** obj+0x50 / +0x11 / +0x12 */
export interface AnimPos {
  raw: number
  frame: number
  tick: number
}

/** 0x0800DE6C: true when the end callback fires (frame stays on the last shown frame) */
export function tickOnce(a: AnimPos, n: number, shift: number): boolean {
  a.tick = (a.tick + 1) & 0xffff
  const f = a.tick >> shift
  if (f >= n) return true
  a.frame = f
  return false
}

/** 0x0800DEB0 */
export function tickLoop(a: AnimPos, n: number, shift: number) {
  a.tick = (a.tick + 1) & 0xffff
  let f = a.tick >> shift
  if (f >= n) {
    a.tick = 0
    f = 0
  }
  a.frame = f
}

/** 0x0800DFD4 */
export function tickIdle(a: AnimPos, n: number) {
  a.tick = (a.tick + 1) & 0xffff
  if (n === 2) {
    a.frame = (a.tick & 0xff) > 0xf0 ? 1 : 0
    return
  }
  a.frame = (a.tick >> 3) & 0xff
  if (a.frame >= n) {
    a.frame = 0
    a.tick = 0
  }
}

/** 0x0800DF8A */
export function tickBored(a: AnimPos, n: number) {
  a.tick = (a.tick + 1) & 0xffff
  const f = a.tick >> 5
  if (f >= n) {
    a.tick = 0x40
    a.frame = 2
    return
  }
  a.frame = f
}
