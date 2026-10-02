// node run.mjs <rom> — runs the wasm32-unknown-unknown build of the determinism digest (headless, no imports).
import { readFileSync } from 'node:fs';
const wasm = readFileSync(new URL('../target/wasm32-unknown-unknown/release/wasm_check.wasm', import.meta.url));
const rom = readFileSync(process.argv[2]);
const { instance } = await WebAssembly.instantiate(wasm, {});
const ex = instance.exports;
const p = ex.alloc(rom.length);
new Uint8Array(ex.memory.buffer, p, rom.length).set(rom);
const out = ex.digest(p, rom.length);
if (!out) { console.error('digest failed'); process.exit(1); }
const v = new BigUint64Array(ex.memory.buffer, out, 2);
console.log(`wasm digest ${v[0].toString(16).padStart(16, '0')} effects ${v[1]}`);
