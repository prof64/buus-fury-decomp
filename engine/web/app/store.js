// Persistent local storage for the user's ROM and the extracted content: Origin Private File System when the
// browser can write it from the main thread, IndexedDB otherwise. Nothing ever leaves the machine.
//
//   const s = await openStore();  await s.put('content/base.tar', bytes);  await s.get(key) -> Uint8Array|null
//   await s.del(key);  await s.keys('mods/') -> [key...];  s.kind -> 'opfs' | 'idb'

async function opfsStore() {
  if (!navigator.storage?.getDirectory) return null;
  if (typeof FileSystemFileHandle === 'undefined' || !('createWritable' in FileSystemFileHandle.prototype)) return null;
  const root = await navigator.storage.getDirectory();
  const dir = await root.getDirectoryHandle('wf-buu', { create: true });
  // keys are flat file names ('/' -> '__')
  const fname = (k) => k.replaceAll('/', '__');
  return {
    kind: 'opfs',
    async get(k) {
      try {
        const h = await dir.getFileHandle(fname(k));
        return new Uint8Array(await (await h.getFile()).arrayBuffer());
      } catch {
        return null;
      }
    },
    async put(k, bytes) {
      const h = await dir.getFileHandle(fname(k), { create: true });
      const w = await h.createWritable();
      await w.write(bytes);
      await w.close();
    },
    async del(k) {
      try {
        await dir.removeEntry(fname(k));
      } catch {}
    },
    async keys(prefix = '') {
      const out = [];
      for await (const name of dir.keys()) {
        const k = name.replaceAll('__', '/');
        if (k.startsWith(prefix)) out.push(k);
      }
      return out;
    },
  };
}

function idbStore() {
  const open = () =>
    new Promise((res, rej) => {
      const r = indexedDB.open('wf-buu', 1);
      r.onupgradeneeded = () => r.result.createObjectStore('files');
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
  const tx = async (mode, fn) => {
    const db = await open();
    return new Promise((res, rej) => {
      const t = db.transaction('files', mode);
      const req = fn(t.objectStore('files'));
      t.oncomplete = () => res(req?.result);
      t.onerror = () => rej(t.error);
    });
  };
  return {
    kind: 'idb',
    async get(k) {
      const v = await tx('readonly', (s) => s.get(k));
      return v ? new Uint8Array(v) : null;
    },
    put: (k, bytes) => tx('readwrite', (s) => s.put(bytes, k)),
    del: (k) => tx('readwrite', (s) => s.delete(k)),
    async keys(prefix = '') {
      const all = await tx('readonly', (s) => s.getAllKeys());
      return all.filter((k) => String(k).startsWith(prefix));
    },
  };
}

export async function openStore() {
  try {
    const o = await opfsStore();
    if (o) return o;
  } catch {}
  return idbStore();
}
