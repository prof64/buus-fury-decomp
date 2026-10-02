// Connects a battle to the audio engine: picks the music the game would play (the arena map's own song from the
// area table; the scripted battle theme when the fight has a boss), maps ruleset sound events to the ripped SFX
// the game fires at the same point (data/<game>/audio.json `events`, found by breakpointing PlaySfx in the
// emulator - tools/audio/AUDIO.md), and runs the HUD's low-HP beep (0x0800BA3A: HP <= maxHP/5, every 61 frames).

import type { BattleSession } from '../core/session'
import { setAudioSink, type AudioEventData, type AudioEventType } from './bus'
import { audio, type AudioCatalog } from './engine'

/** skills whose B press starts the charge loop (SFX 3): beams, Big Bang, Energy Sword, Final Flash, Masenko */
const CHARGE = new Set([1, 2, 3, 6, 9, 10, 11, 15, 16, 17])
/** beams that play the beam-end sound (33) when they stop (beam object 0x080207A0) */
const BEAM_END = new Set([1, 3, 6, 15, 16])

interface HpLike {
  kind: string
  hp?: number
  maxHp?: number
}

/** the arena map's own song (area record +0x24): the game keeps the area music for its boss fights, except the few
 *  story fights whose script plays the battle theme (catalog boss_music_types: Majin Buu at West City, the World
 *  Tournament ring); a catalog without that list falls back to "any boss" */
function pickMusic(s: BattleSession, cat: AudioCatalog): number | null {
  const types = cat.boss_music_types
  const ids = s.cfg.phases.flatMap((p) => p.enemy.split(/[+~]/).filter(Boolean))
  if (types && cat.boss_music != null) {
    if (ids.some((t) => types.includes(Number(t.replace(/^t/, ''))))) return cat.boss_music
  } else {
    const enemies = s.ruleset.enemies()
    if (ids.some((t) => enemies.find((e) => e.id === t || e.id === '~' + t)?.group === 'Boss') && cat.boss_music != null) return cat.boss_music
  }
  return cat.arena_music[s.cfg.arena] ?? null
}

/** Returns a disposer. No-op for games without a ripped audio catalog. */
export function attachBattleAudio(s: BattleSession): () => void {
  const game = s.cfg.game
  let cat: AudioCatalog | null = null
  let disposed = false
  let skill = -1
  let beamOn = false
  let lastBeep = -1000
  let lastHitFrame = -1
  let wasPaused = s.paused
  const offGesture = audio.unlockOnGesture()
  audio.unlock()

  const play = (ev: string, loopKey?: string) => {
    const e = cat?.events[ev]
    if (!e || !e.sfx.length) return
    const id = e.sfx[Math.floor(Math.random() * e.sfx.length)] // not the game RNG: sound never changes the fight
    const vol = e.volume ?? cat!.sfx.find((x) => x.id === id)?.game_volume ?? 64
    void audio.playSfx(game, id, vol, loopKey)
  }

  // exact driver calls (sfx / sfx_stop / song, emitted by the Buu ruleset at the game's call sites): every play
  // gets its own key so that StopSfx(id) stops every voice of that effect, like the driver (0x0803E302)
  const voices = new Map<number, string[]>()
  let serial = 0
  let lastImpactFrame = -1
  const exact = (type: AudioEventType, data?: AudioEventData): boolean => {
    if (!cat) return true
    if (type === 'sfx' && data?.id != null) {
      const key = `sfx:${data.id}:${serial++}`
      const list = (voices.get(data.id) ?? []).slice(-15)
      list.push(key)
      voices.set(data.id, list)
      if (data.id === 48 || data.id === 49) lastImpactFrame = s.world.frame
      void audio.playSfx(game, data.id, data.vol ?? 64, key)
      return true
    }
    if (type === 'sfx_stop' && data?.id != null) {
      for (const k of voices.get(data.id) ?? []) audio.stopLoop(k)
      voices.delete(data.id)
      return true
    }
    if (type === 'song' && data?.id != null) {
      void audio.playMusic(game, data.id)
      return true
    }
    return false
  }

  const sink = (type: AudioEventType, data?: AudioEventData) => {
    if (!cat) return
    if (exact(type, data)) return
    switch (type) {
      case 'melee_swing':
      case 'dash':
      case 'level_up':
      case 'ki_blast':
        play(type)
        break
      case 'hit':
        // fallback for attacks whose object does not emit its own impact sound yet: one impact per game frame
        // (beams / Flame Kamehameha land several hits a frame), none when the attacker already played 48 / 49; the
        // Dragon Fist's area hits make no impact sound in the game
        if (s.world.frame === lastHitFrame || s.world.frame === lastImpactFrame || skill === 5) break
        lastHitFrame = s.world.frame
        play('hit')
        break
      case 'transform':
        play('transform', 'transform')
        break
      case 'transform_done':
        audio.stopLoop('transform')
        play('transform')
        break
      case 'hurt':
        play('hit')
        if (data?.melee) play('melee_swing')
        break
      case 'skill_start':
        // the Buu player emits its skills' exact sounds (sfx events); this only tracks the skill
        skill = data?.id ?? -1
        if (game !== 'buu') {
          if (CHARGE.has(skill)) play('charge', 'charge')
          else if (skill === 5) play('dragon_fist')
          else if (skill === 8) play('energy_punch')
        }
        break
      case 'beam_fire':
        audio.stopLoop('charge')
        play('beam', 'beam')
        beamOn = true
        break
      case 'wave':
        audio.stopLoop('charge')
        play('wave')
        break
      case 'big_bang':
        audio.stopLoop('charge')
        play('big_bang')
        break
      case 'skill_end':
        if (game === 'buu') {
          skill = -1
          break
        }
        audio.stopLoop('charge')
        if (beamOn) {
          audio.stopLoop('beam')
          if (BEAM_END.has(skill)) play('beam_end')
        }
        beamOn = false
        skill = -1
        break
    }
  }
  setAudioSink(sink)

  void audio.catalog(game).then((c) => {
    if (disposed || !c) return
    cat = c
    const m = pickMusic(s, c)
    if (m != null) void audio.playMusic(game, m)
    // the battle sounds: the event table plus everything the Buu player emits as exact driver calls
    const ids = new Set([...Object.values(c.events).flatMap((e) => e.sfx), 3, 20, 21, 23, 24, 25, 31, 32, 33, 34, 36, 41, 46, 47, 48, 49, 50, 51, 54, 59])
    audio.preload(c.sfx.filter((x) => ids.has(x.id)).map((x) => x.file))
  })

  // per displayed frame: pause toggles (menu sound). The low-HP beep is the ruleset's (Buu: HUD 0x0800BA3A emits
  // sound 41 at volume 32 itself); for a ruleset that does not, the beep is timed here in game frames.
  let raf = 0
  const ownBeep = game === 'buu'
  const tick = () => {
    raf = requestAnimationFrame(tick)
    if (!cat) return
    if (s.paused !== wasPaused) {
      wasPaused = s.paused
      audio.stopAllSfx()
      voices.clear()
      beamOn = false
      play('menu')
    }
    if (s.paused || s.outcome !== 'running' || ownBeep) return
    const p = s.world.entities.find((e) => e.kind === 'player') as unknown as HpLike | undefined
    const f = s.world.frame
    if (p && p.hp != null && p.maxHp != null && p.hp > 0 && 5 * p.hp <= p.maxHp && f - lastBeep > 60) {
      lastBeep = f
      play('low_hp')
    }
  }
  raf = requestAnimationFrame(tick)

  return () => {
    disposed = true
    cancelAnimationFrame(raf)
    offGesture()
    setAudioSink(null)
    audio.stopAllSfx()
    audio.stopMusic()
  }
}
