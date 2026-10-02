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

function pickMusic(s: BattleSession, cat: AudioCatalog): number | null {
  const enemies = s.ruleset.enemies()
  const isBoss = (id: string) =>
    id
      .split(/[+~]/)
      .filter(Boolean)
      .some((t) => enemies.find((e) => e.id === t || e.id === '~' + t)?.group === 'Boss')
  if (s.cfg.phases.some((p) => isBoss(p.enemy)) && cat.boss_music != null) return cat.boss_music
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

  const sink = (type: AudioEventType, data?: AudioEventData) => {
    if (!cat) return
    switch (type) {
      case 'melee_swing':
      case 'dash':
      case 'level_up':
      case 'ki_blast':
        play(type)
        break
      case 'hit':
        // one impact per game frame (beams / Flame Kamehameha land several hits a frame); the Dragon Fist's
        // area hits make no impact sound in the game
        if (s.world.frame === lastHitFrame || skill === 5) break
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
        skill = data?.id ?? -1
        if (CHARGE.has(skill)) play('charge', 'charge')
        else if (skill === 5) play('dragon_fist')
        else if (skill === 8) play('energy_punch')
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
    const ids = new Set(Object.values(c.events).flatMap((e) => e.sfx))
    audio.preload(c.sfx.filter((x) => ids.has(x.id)).map((x) => x.file))
  })

  // per displayed frame: pause toggles (menu sound) and the low-HP beep, timed in game frames
  let raf = 0
  const tick = () => {
    raf = requestAnimationFrame(tick)
    if (!cat) return
    if (s.paused !== wasPaused) {
      wasPaused = s.paused
      audio.stopAllSfx()
      beamOn = false
      play('menu')
    }
    if (s.paused || s.outcome !== 'running') return
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
