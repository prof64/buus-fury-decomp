// Run the wasm32 build under Node and compare its output with the native CLI's.
//   node engine/web/node_check.mjs <wf_wasm.wasm> <rom.gba> [native base dir]
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { loadExtractor } from './extract.js';

const [wasmPath, romPath, nativeDir] = process.argv.slice(2);
const ex = await loadExtractor(readFileSync(wasmPath));
const t0 = performance.now();
const files = ex.extract(new Uint8Array(readFileSync(romPath)));
const ms = performance.now() - t0;
let bytes = 0;
for (const v of files.values()) bytes += v.length;
console.log(`wasm: ${files.size} files, ${(bytes / 1e6).toFixed(1)} MB in ${(ms / 1000).toFixed(2)} s`);
if (nativeDir) {
  let same = 0, diff = [], missing = [];
  for (const [k, v] of files) {
    const p = join(nativeDir, k);
    if (!existsSync(p)) { missing.push(k); continue; }
    const n = readFileSync(p);
    if (n.length === v.length && n.equals(Buffer.from(v))) same++; else diff.push(k);
  }
  console.log(`vs native: ${same} identical, ${diff.length} differ, ${missing.length} missing`);
  for (const k of [...diff, ...missing].slice(0, 10)) console.log('  ' + k);
  if (diff.some((k) => k !== 'pack.json') || missing.length) process.exit(1);
}
try { ex.extract(new Uint8Array(1024)); console.log('wrong ROM accepted?!'); process.exit(1); }
catch (e) { console.log('wrong ROM rejected:', e.message.slice(0, 60) + '...'); }
