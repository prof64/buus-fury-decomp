// wasm32 side of the native == wasm mod check (crates/wf-modcheck): feeds the base pack and the mod folders to the
// wasm module, runs the same scenarios as the native example and prints the digests (compare with
//   cargo run --release -p wf-modcheck --example digest -- <content/base> <mod>...).
//   cargo build -p wf-modcheck --target wasm32-unknown-unknown --profile release-wasm
//   node engine/web/mods_check.mjs engine/target/wasm32-unknown-unknown/release-wasm/wf_modcheck.wasm <content/base> <mod>... [--frames N] [--expect <native output file>]
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative } from 'node:path'

const args = process.argv.slice(2)
let frames = 1200
let expect = null
const fi = args.indexOf('--frames')
if (fi >= 0) frames = Number(args.splice(fi, 2)[1])
const ei = args.indexOf('--expect')
if (ei >= 0) expect = readFileSync(args.splice(ei, 2)[1], 'utf8')
const [wasmPath, ...dirs] = args
const { instance } = await WebAssembly.instantiate(readFileSync(wasmPath), {})
const x = instance.exports
const mem = () => new Uint8Array(x.memory.buffer)
const put = (bytes) => {
  const p = x.mc_alloc(bytes.length)
  mem().set(bytes, p)
  return p
}
const walk = (root, dir, out) => {
  for (const n of readdirSync(dir).sort()) {
    const p = join(dir, n)
    if (statSync(p).isDirectory()) walk(root, p, out)
    else out.push([relative(root, p).split('\\').join('/'), p])
  }
  return out
}
dirs.forEach((d, pack) => {
  for (const [rel, p] of walk(d, d, [])) {
    const path = new TextEncoder().encode(rel)
    const data = readFileSync(p)
    x.mc_file(pack, put(path), path.length, put(data), data.length)
  }
})
const t = performance.now()
const code = x.mc_run(frames)
const text = new TextDecoder().decode(mem().slice(x.mc_result_ptr(), x.mc_result_ptr() + x.mc_result_len()))
console.log(text.trimEnd())
console.error(`wasm: ${Math.round(performance.now() - t)} ms`)
if (code !== 0) process.exit(1)
if (expect !== null) {
  const same = expect.trim() === text.trim()
  console.log(same ? 'native == wasm: identical' : 'native != wasm: DIFFERENT')
  if (!same) process.exit(1)
}
