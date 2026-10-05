// Shared helpers for the headless browser checks: a muted headless Chromium (never a window, no sound) on a
// fresh profile, the static server, and small page helpers.
import { chromium } from 'playwright-core';
import { existsSync, readdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir, homedir } from 'node:os';
import { serve } from './serve.mjs';

/** the Playwright-managed headless shell (npx playwright install chromium-headless-shell), else $CHROMIUM */
export function chromiumPath() {
  if (process.env.CHROMIUM) return process.env.CHROMIUM;
  const base = join(homedir(), 'Library/Caches/ms-playwright');
  const alt = join(homedir(), '.cache/ms-playwright');
  for (const root of [base, alt]) {
    if (!existsSync(root)) continue;
    for (const d of readdirSync(root).filter((d) => d.startsWith('chromium_headless_shell-')).sort().reverse()) {
      for (const sub of ['chrome-headless-shell-mac-arm64/chrome-headless-shell', 'chrome-headless-shell-mac-x64/chrome-headless-shell', 'chrome-headless-shell-linux64/chrome-headless-shell']) {
        const p = join(root, d, sub);
        if (existsSync(p)) return p;
      }
    }
  }
  return undefined; // playwright's own lookup
}

/** headless + muted browser and a persistent profile dir (so a page reload / a second page keeps OPFS / storage) */
export async function launch() {
  const profile = mkdtempSync(join(tmpdir(), 'wf-web-'));
  const ctx = await chromium.launchPersistentContext(profile, {
    headless: true,
    executablePath: chromiumPath(),
    args: ['--mute-audio', '--autoplay-policy=no-user-gesture-required', '--disable-background-timer-throttling', '--disable-renderer-backgrounding'],
    viewport: { width: 960, height: 640 },
  });
  return { ctx, profile };
}

export { serve };

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function waitFor(page, fn, arg, { timeout = 600000, poll = 250, what = 'condition' } = {}) {
  const t0 = Date.now();
  for (;;) {
    const v = await page.evaluate(fn, arg).catch(() => undefined);
    if (v) return v;
    if (Date.now() - t0 > timeout) throw new Error(`timed out waiting for ${what}`);
    await sleep(poll);
  }
}

/** hold a key for about `frames` game frames (wall time at 59.73 fps) */
export async function hold(page, code, frames) {
  await page.keyboard.down(code);
  await sleep((frames * 1000) / 59.7275);
  await page.keyboard.up(code);
}
export async function tap(page, code, after = 10) {
  await hold(page, code, 3);
  await sleep((after * 1000) / 59.7275);
}

/** the app's status JSON (frame, recording, ...) */
export const status = (page) => page.evaluate(() => window.wf && window.wf.status());

/** a blob export of the game wasm as bytes (Node Buffer): `fn` is the export name, 0 = ok */
export async function blobOf(page, fn) {
  const b64 = await page.evaluate((fn) => {
    const x = window.wf.x;
    if (x[fn]()) return null;
    const b = new Uint8Array(x.memory.buffer, x.wf_blob_ptr(), x.wf_blob_len()).slice();
    let s = '';
    for (let i = 0; i < b.length; i += 0x8000) s += String.fromCharCode(...b.subarray(i, i + 0x8000));
    return btoa(s);
  }, fn);
  return b64 ? Buffer.from(b64, 'base64') : null;
}

export function save(dir, name, bytes) {
  const p = join(dir, name);
  writeFileSync(p, bytes);
  return p;
}
