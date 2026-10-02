// Extraction off the main thread: ROM bytes in, one tar of the base pack out, progress per category.
//   post {rom: Uint8Array (transferred), wasmUrl}  ->  {type:'progress', done, total, category, ms}
//                                                   ->  {type:'done', tar, ms, files, bytes} | {type:'error', message}
import { loadExtractor } from '../extract.js';
import { tar } from './tar.js';

const CATEGORIES = ['tables', 'sprites', 'effects', 'portraits', 'items', 'maps', 'audio'];

self.onmessage = async (e) => {
  const { rom, wasmUrl } = e.data;
  try {
    const t0 = performance.now();
    const bytes = new Uint8Array(await (await fetch(wasmUrl)).arrayBuffer());
    const ex = await loadExtractor(bytes);
    const all = new Map();
    const packs = [];
    for (let i = 0; i < CATEGORIES.length; i++) {
      const c = CATEGORIES[i];
      self.postMessage({ type: 'progress', done: i, total: CATEGORIES.length, category: c, ms: performance.now() - t0 });
      const files = ex.extract(rom, c);
      packs.push(JSON.parse(new TextDecoder().decode(files.get('pack.json'))));
      for (const [k, v] of files) if (k !== 'pack.json' && k !== 'files.json') all.set(k, v);
    }
    // one manifest for the merged categories
    const pack = packs[packs.length - 1];
    pack.files_per_category = Object.fromEntries(CATEGORIES.map((c, i) => [c, packs[i].files_per_category[c]]));
    pack.warnings = packs.flatMap((p) => p.warnings || []);
    const enc = new TextEncoder();
    all.set('files.json', enc.encode(JSON.stringify([...all.keys(), 'pack.json', 'files.json'].sort())));
    all.set('pack.json', enc.encode(JSON.stringify(pack, null, 2)));
    const t = tar(new Map([...all.entries()].sort((a, b) => (a[0] < b[0] ? -1 : 1))));
    let size = 0;
    for (const v of all.values()) size += v.length;
    self.postMessage({ type: 'done', tar: t, ms: performance.now() - t0, files: all.size, bytes: size }, [t.buffer]);
  } catch (err) {
    self.postMessage({ type: 'error', message: String(err?.message || err) });
  }
};
