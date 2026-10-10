// A static server for engine/web/ plus the local ROM at /rom.gba (same origin, for the app's ?devrom= hook).
// Used by browser_check.mjs; also runnable alone: node serve.mjs [port] [rom]
import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { join, extname, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
export const WEB = resolve(HERE, '..');
export const REPO = resolve(HERE, '../../..');
const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.gba': 'application/octet-stream',
};

export function serve({ port = 0, rom = join(REPO, 'roms/buu.gba') } = {}) {
  const server = createServer(async (req, res) => {
    const url = new URL(req.url, 'http://x');
    let path = decodeURIComponent(url.pathname);
    let file = path === '/rom.gba' ? rom : join(WEB, path);
    if (!file.startsWith(WEB) && file !== rom) {
      res.writeHead(403).end();
      return;
    }
    try {
      if ((await stat(file)).isDirectory()) file = join(file, 'index.html');
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((ok) => server.listen(port, '127.0.0.1', () => ok({ server, url: `http://127.0.0.1:${server.address().port}` })));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const s = await serve({ port: Number(process.argv[2] || 5370), rom: process.argv[3] });
  console.log(`serving ${WEB} at ${s.url}/app/ (ROM at /rom.gba)`);
}
