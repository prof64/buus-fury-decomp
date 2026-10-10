// Headless check of the wasm build's simulation exports (no browser, no audio):
//   node engine/web/core_check.mjs engine/target/wasm32-unknown-unknown/release-wasm/wf_wasm.wasm roms/buu.gba
import { readFileSync } from 'node:fs'

const [wasmPath, romPath] = process.argv.slice(2)
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {})
const x = instance.exports
const mem = () => new Uint8Array(x.memory.buffer)
const put = (bytes) => {
  const p = x.wf_alloc(bytes.length)
  mem().set(bytes, p)
  return p
}
const result = () => mem().slice(x.wf_result_ptr(), x.wf_result_ptr() + x.wf_result_len())
const text = () => new TextDecoder().decode(result())

const rom = readFileSync(romPath)
const only = new TextEncoder().encode('tables,sprites,effects,maps')
let t = performance.now()
const st = x.wf_extract(put(rom), rom.length, put(only), only.length)
console.log('extract', st, Math.round(performance.now() - t), 'ms')
console.log('load', x.wf_core_load_last())
const cfg = new TextEncoder().encode(JSON.stringify({ arena: 'forest', players: [{ fighter: 'goku', form: 2, level: 60 }], phases: [{ enemy: 't68', count: 4 }], rng: { Seed: 7 } }))
console.log('reset', x.wf_core_reset(put(cfg), cfg.length), text().slice(0, 200))
const keys = x.wf_alloc(8)
t = performance.now()
let events = 0
for (let f = 0; f < 1200; f++) {
  new Uint16Array(x.memory.buffer, keys, 1)[0] = [0, 0x10, 0x20, 1, 0x40][(f >> 4) % 5]
  events += x.wf_core_step(keys, 1)
}
console.log('1200 frames', Math.round(performance.now() - t), 'ms; events', events, 'hash', (x.wf_core_hash_hi() >>> 0).toString(16), (x.wf_core_hash_lo() >>> 0).toString(16))
x.wf_core_observe(0)
const o = JSON.parse(text())
console.log('frame', o.frame, 'player hp', o.me.hp, 'enemies', o.enemies.length, o.enemies.map((e) => e.ai.at(-1)))
