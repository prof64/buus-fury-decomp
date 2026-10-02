// Web Audio playback of the ripped game audio (data/<game>/audio.json, files under site/public/assets/<game>/audio).
// Music: one track at a time, looped sample-exactly with the catalog's loop points. SFX: like the game's driver,
// 16 voices, first free voice wins and a request is dropped when all are busy; looping effects (charge, beam)
// keep a handle so they can be stopped. Settings (mute / volumes) persist in localStorage; the AudioContext is
// resumed on the first user gesture (browsers refuse to start audio before one).

import { assetUrl } from '../core/assets'
import { loadData } from '../../data'

export interface AudioLoop {
  start: number
  end: number
}

export interface MusicTrack {
  id: number
  name: string
  file: string
  duration: number
  loop: AudioLoop | null
  context?: string
  confidence?: string
}

export interface SfxEntry {
  id: number
  name: string
  file: string
  duration: number
  loop: AudioLoop | null
  game_volume: number
  trigger?: string
}

export interface AudioCatalog {
  music: MusicTrack[]
  sfx: SfxEntry[]
  arena_music: Record<string, number>
  boss_music: number
  /** enemy types whose story fight plays boss_music (the others keep the area's own song) */
  boss_music_types?: number[]
  events: Record<string, { sfx: number[]; loop?: boolean; volume?: number }>
}

export interface AudioSettings {
  muted: boolean
  /** master 0..1 */
  volume: number
  music: number
  sfx: number
}

const KEY = 'bt-audio'
const VOICES = 16

function readSettings(): AudioSettings {
  const d: AudioSettings = { muted: false, volume: 0.7, music: 0.8, sfx: 1 }
  try {
    return { ...d, ...JSON.parse(localStorage.getItem(KEY) ?? '{}') }
  } catch {
    return d
  }
}

type Listener = () => void

export class AudioEngine {
  ctx: AudioContext | null = null
  settings: AudioSettings = typeof localStorage !== 'undefined' ? readSettings() : { muted: true, volume: 0, music: 0, sfx: 0 }
  private master: GainNode | null = null
  private musicGain: GainNode | null = null
  private sfxGain: GainNode | null = null
  private buffers = new Map<string, Promise<AudioBuffer | null>>()
  private catalogs = new Map<string, Promise<AudioCatalog | null>>()
  private music: { src: AudioBufferSourceNode; file: string } | null = null
  private wantMusic: { game: string; id: number } | null = null
  private voices = 0
  private loops = new Map<string, { src: AudioBufferSourceNode; gain: GainNode }>()
  private listeners = new Set<Listener>()
  /** counters for automation / debugging (window.__audio) */
  stats = { sfxPlayed: 0, sfxDropped: 0, musicStarted: 0, lastSfx: -1, lastMusic: -1, errors: 0 }

  catalog(game: string): Promise<AudioCatalog | null> {
    let p = this.catalogs.get(game)
    if (!p) {
      p = (loadData(game, 'audio') as Promise<AudioCatalog | undefined>).then((c) => c ?? null).catch(() => null)
      this.catalogs.set(game, p)
    }
    return p
  }

  subscribe(l: Listener) {
    this.listeners.add(l)
    return () => void this.listeners.delete(l)
  }

  private emit() {
    for (const l of this.listeners) l()
  }

  /** Create / resume the context. Call from a user gesture; safe to call repeatedly. */
  unlock() {
    if (typeof window === 'undefined' || typeof AudioContext === 'undefined') return
    if (!this.ctx) {
      this.ctx = new AudioContext()
      this.master = this.ctx.createGain()
      this.musicGain = this.ctx.createGain()
      this.sfxGain = this.ctx.createGain()
      this.musicGain.connect(this.master)
      this.sfxGain.connect(this.master)
      this.master.connect(this.ctx.destination)
      this.applyGains()
      this.ctx.onstatechange = () => this.emit()
    }
    if (this.ctx.state === 'suspended') void this.ctx.resume().then(() => this.resumeMusic())
    else this.resumeMusic()
  }

  /** Resume on the first pointer / key gesture (the setup screen's Start click usually counts already). */
  unlockOnGesture(): () => void {
    const h = () => {
      this.unlock()
      if (this.ctx?.state === 'running') off()
    }
    const off = () => {
      window.removeEventListener('pointerdown', h, true)
      window.removeEventListener('keydown', h, true)
    }
    window.addEventListener('pointerdown', h, true)
    window.addEventListener('keydown', h, true)
    return off
  }

  get running() {
    return this.ctx?.state === 'running'
  }

  setSettings(p: Partial<AudioSettings>) {
    this.settings = { ...this.settings, ...p }
    try {
      localStorage.setItem(KEY, JSON.stringify(this.settings))
    } catch {
      /* private mode */
    }
    this.applyGains()
    this.emit()
  }

  private applyGains() {
    if (!this.ctx || !this.master) return
    const s = this.settings
    const t = this.ctx.currentTime
    this.master.gain.setTargetAtTime(s.muted ? 0 : s.volume, t, 0.01)
    this.musicGain!.gain.setTargetAtTime(s.music, t, 0.01)
    this.sfxGain!.gain.setTargetAtTime(s.sfx, t, 0.01)
  }

  private buffer(file: string): Promise<AudioBuffer | null> {
    let p = this.buffers.get(file)
    if (!p) {
      const ctx = this.ctx
      if (!ctx) return Promise.resolve(null)
      p = fetch(assetUrl(file))
        .then((r) => {
          if (!r.ok) throw new Error(`${r.status} ${file}`)
          return r.arrayBuffer()
        })
        .then((b) => ctx.decodeAudioData(b))
        .catch((e) => {
          this.stats.errors++
          console.warn('audio: cannot load', file, e)
          this.buffers.delete(file)
          return null
        })
      this.buffers.set(file, p)
    }
    return p
  }

  /** Warm the decode cache (e.g. the battle's SFX) so the first hit is not late. */
  preload(files: string[]) {
    if (!this.ctx) return
    for (const f of files) void this.buffer(f)
  }

  // ---- music ------------------------------------------------------------------------------------------------
  async playMusic(game: string, id: number) {
    this.wantMusic = { game, id }
    if (!this.ctx) return
    const cat = await this.catalog(game)
    const t = cat?.music.find((m) => m.id === id)
    if (!t || this.wantMusic?.id !== id) return
    if (this.music?.file === t.file) return
    const buf = await this.buffer(t.file)
    if (!buf || !this.ctx || this.wantMusic?.id !== id || this.wantMusic.game !== game) return
    this.stopMusicNode()
    const src = this.ctx.createBufferSource()
    src.buffer = buf
    if (t.loop) {
      src.loop = true
      src.loopStart = t.loop.start
      src.loopEnd = Math.min(t.loop.end, buf.duration)
    }
    src.connect(this.musicGain!)
    src.start()
    src.onended = () => {
      if (this.music?.src === src) this.music = null
    }
    this.music = { src, file: t.file }
    this.stats.musicStarted++
    this.stats.lastMusic = id
    this.emit()
  }

  private resumeMusic() {
    if (this.wantMusic && !this.music) void this.playMusic(this.wantMusic.game, this.wantMusic.id)
  }

  private stopMusicNode() {
    if (this.music) {
      try {
        this.music.src.stop()
      } catch {
        /* already stopped */
      }
      this.music = null
    }
  }

  stopMusic() {
    this.wantMusic = null
    this.stopMusicNode()
    this.emit()
  }

  get musicId(): number | null {
    return this.wantMusic?.id ?? null
  }

  // ---- sound effects ----------------------------------------------------------------------------------------
  /** Play a sound effect. volume is the game's 0..64 value. `loopKey` keeps a looping effect running until stopLoop. */
  async playSfx(game: string, id: number, volume = 64, loopKey?: string) {
    if (!this.ctx || this.ctx.state !== 'running' || this.settings.muted) return
    if (this.voices >= VOICES) {
      this.stats.sfxDropped++
      return
    }
    const cat = await this.catalog(game)
    const e = cat?.sfx.find((s) => s.id === id)
    if (!e) return
    this.voices++
    const buf = await this.buffer(e.file)
    if (!buf || !this.ctx) {
      this.voices--
      return
    }
    if (loopKey) this.stopLoop(loopKey)
    const src = this.ctx.createBufferSource()
    src.buffer = buf
    const g = this.ctx.createGain()
    g.gain.value = Math.max(0, Math.min(1, volume / 64))
    src.connect(g).connect(this.sfxGain!)
    if (loopKey && e.loop) {
      src.loop = true
      src.loopStart = e.loop.start
      src.loopEnd = Math.min(e.loop.end, buf.duration)
    }
    src.onended = () => {
      this.voices--
      if (loopKey && this.loops.get(loopKey)?.src === src) this.loops.delete(loopKey)
    }
    src.start()
    if (loopKey) this.loops.set(loopKey, { src, gain: g })
    this.stats.sfxPlayed++
    this.stats.lastSfx = id
  }

  stopLoop(key: string) {
    const l = this.loops.get(key)
    if (!l) return
    this.loops.delete(key)
    try {
      l.src.stop()
    } catch {
      /* not started */
    }
  }

  stopAllSfx() {
    for (const k of [...this.loops.keys()]) this.stopLoop(k)
  }
}

export const audio = new AudioEngine()

declare global {
  interface Window {
    __audio?: AudioEngine
  }
}
if (typeof window !== 'undefined') window.__audio = audio
