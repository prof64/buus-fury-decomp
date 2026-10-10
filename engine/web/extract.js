// Browser / Node glue for the wasm extractor (crates/wf-wasm). No wasm-bindgen: plain exports.
//
//   import { loadExtractor } from './extract.js';
//   const ex = await loadExtractor(fetch('wf_wasm.wasm'));      // or bytes (Node)
//   const files = ex.extract(romUint8Array, 'sprites,maps');   // Map<'sprites/goku/sheet.png', Uint8Array>
//
// extract() throws on a wrong ROM (message carries the SHA-1 found). The wasm returns a ustar archive of
// `base/<path>` entries; untar() below turns it into a Map keyed by the path under base/.

export async function loadExtractor(source) {
  const imports = {};
  const { instance } =
    source instanceof Promise || (typeof Response !== 'undefined' && source instanceof Response)
      ? await WebAssembly.instantiateStreaming(source, imports)
      : await WebAssembly.instantiate(source, imports);
  const x = instance.exports;
  return {
    extract(rom, only = '') {
      const ptr = x.wf_alloc(rom.length);
      new Uint8Array(x.memory.buffer, ptr, rom.length).set(rom);
      const o = new TextEncoder().encode(only);
      const optr = o.length ? x.wf_alloc(o.length) : 0;
      if (o.length) new Uint8Array(x.memory.buffer, optr, o.length).set(o);
      const status = x.wf_extract(ptr, rom.length, optr, o.length);
      const out = new Uint8Array(x.memory.buffer, x.wf_result_ptr(), x.wf_result_len()).slice();
      x.wf_free_result();
      if (status !== 0) throw new Error(new TextDecoder().decode(out));
      return untar(out);
    },
  };
}

export function untar(buf) {
  const files = new Map();
  const dec = new TextDecoder();
  const str = (a, b) => dec.decode(buf.subarray(a, b)).replace(/\0.*$/s, '');
  for (let p = 0; p + 512 <= buf.length; ) {
    const name = str(p, p + 100);
    if (!name) break;
    const prefix = str(p + 345, p + 500);
    const size = parseInt(str(p + 124, p + 136).trim(), 8) || 0;
    const full = prefix ? `${prefix}/${name}` : name;
    files.set(full.replace(/^base\//, ''), buf.subarray(p + 512, p + 512 + size));
    p += 512 + Math.ceil(size / 512) * 512;
  }
  return files;
}
