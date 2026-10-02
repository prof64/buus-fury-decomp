// Minimal ustar reader / writer: one content pack <-> one blob (what we cache in OPFS / IndexedDB).
import { untar as untarBase } from '../extract.js';

/** Extraction categories, in order (keep in sync with wf_extract::CATEGORIES). Part of the content cache key. */
export const CATEGORIES = ['tables', 'sprites', 'effects', 'portraits', 'items', 'maps', 'audio', 'ui', 'world', 'scripts'];

/** Map<path, Uint8Array> from a ustar archive; strips a leading `base/` (the extractor's prefix). */
export function untar(buf) {
  return untarBase(buf);
}

/** ustar archive of a Map<path, Uint8Array> (paths stored as-is, long names through the prefix field). */
export function tar(files) {
  let total = 1024;
  for (const [, d] of files) total += 512 + Math.ceil(d.length / 512) * 512;
  const out = new Uint8Array(total);
  const enc = new TextEncoder();
  let p = 0;
  const put = (s, off, len) => {
    const b = enc.encode(s);
    out.set(b.subarray(0, len), p + off);
  };
  for (const [path, data] of files) {
    let name = path;
    let prefix = '';
    if (enc.encode(path).length > 100) {
      const cut = path.length - 100;
      const i = path.indexOf('/', cut);
      prefix = path.slice(0, i);
      name = path.slice(i + 1);
    }
    put(name, 0, 100);
    put('0000644\0', 100, 8);
    put('0000000\0', 108, 8);
    put('0000000\0', 116, 8);
    put(data.length.toString(8).padStart(11, '0') + '\0', 124, 12);
    put('00000000000\0', 136, 12);
    out[p + 156] = 0x30;
    put('ustar\0', 257, 6);
    put('00', 263, 2);
    put(prefix, 345, 155);
    put('        ', 148, 8);
    let sum = 0;
    for (let i = 0; i < 512; i++) sum += out[p + i];
    put(sum.toString(8).padStart(6, '0') + '\0 ', 148, 8);
    out.set(data, p + 512);
    p += 512 + Math.ceil(data.length / 512) * 512;
  }
  return out;
}
