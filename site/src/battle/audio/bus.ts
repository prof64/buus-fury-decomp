// Sound event bus. Rulesets call `audioEvent()` at the points where the real game calls its PlaySfx / PlaySong
// (tools/audio/AUDIO.md); the battle view connects a sink that turns them into sound. With no sink (tests,
// headless runs) it is a no-op, and it never touches the game RNG.

export interface AudioEventData {
  /** skill id for skill_start */
  id?: number
  /** the hit came from a melee strike (the game adds the swing sound 50/51) */
  melee?: boolean
  /** sfx: the game's volume 0..64 */
  vol?: number
}

export type AudioEventType =
  | 'melee_swing' // player A press (attack-type callback 0x0800DEE8: SFX 50/51)
  | 'hit' // damage landed on an enemy (SFX 48/49)
  | 'hurt' // damage landed on the player (48/49, plus 50/51 for melee)
  | 'dash' // SFX 59
  | 'level_up' // SFX 36 (script #3)
  | 'skill_start' // charge loop 3 / Dragon Fist 24 / Energy Punch 47 by skill id
  | 'skill_end' // stops the charge / beam loops (beam: SFX 33)
  | 'ki_blast' // SFX 20/21
  | 'beam_fire' // stop charge, start beam loop 32
  | 'wave' // stop charge, SFX 34 (Burning Attack, Masenko, Ultra Volleyball)
  | 'big_bang' // stop charge, SFX 23
  | 'transform' // SFX 54, looping while the 211-frame transformation runs (0x0801C414)
  | 'transform_done' // SFX 54 once when the new form is applied (0x0800E3CC; also on revert)
  // exact driver calls (the player's states emit these at the game's call sites, verified by the lockstep harness)
  | 'sfx' // PlaySfx 0x0803E378: {id, vol}
  | 'sfx_stop' // StopSfx 0x0803E302: {id}
  | 'sfx_stop_match' // StopMatching 0x0803E32C: {id} (stops the voices playing that effect)
  | 'sfx_stop_all' // StopAll 0x0803E2EC
  | 'song' // PlaySong 0x0803F110: {id}

export type AudioSink = (type: AudioEventType, data?: AudioEventData) => void

let sink: AudioSink | null = null

export function audioEvent(type: AudioEventType, data?: AudioEventData) {
  if (sink) sink(type, data)
}

export function setAudioSink(s: AudioSink | null) {
  sink = s
}
