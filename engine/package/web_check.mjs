// Headless smoke test of the web build (headless Chrome, muted, via the DevTools protocol; no npm deps):
//   python3 package/serve.py --dir <dir with dist + rom.gba> &  node package/web_check.mjs http://localhost:8370 out/
// Drops the ROM through the ?devrom= hook, waits for the launcher, screenshots it, then drives
// Battle -> START and Story -> New Game and checks the game runs (frames advance) in each.
import { spawn } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [base = 'http://localhost:8370', outDir = '.'] = process.argv.slice(2);
const CHROME = process.env.CHROME || '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const port = 9300 + Math.floor(Math.random() * 500);
const profile = mkdtempSync(join(tmpdir(), 'wf-chrome-'));
const chrome = spawn(CHROME, ['--headless=new', '--mute-audio', '--no-first-run', '--no-default-browser-check', `--user-data-dir=${profile}`, `--remote-debugging-port=${port}`, '--window-size=960,640', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let ws;
let id = 0;
const pending = new Map();
const send = (method, params = {}) =>
  new Promise((res, rej) => {
    const k = ++id;
    pending.set(k, { res, rej });
    ws.send(JSON.stringify({ id: k, method, params }));
  });
const evalJs = async (expr) => (await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true })).result.value;
const until = async (expr, ms, what) => {
  const t = Date.now();
  while (Date.now() - t < ms) {
    if (await evalJs(expr).catch(() => false)) return;
    await sleep(250);
  }
  throw new Error(`timeout: ${what}`);
};
const shot = async (name) => {
  const { data } = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(outDir, name), Buffer.from(data, 'base64'));
  console.log('wrote', join(outDir, name));
};

try {
  let target;
  for (let k = 0; k < 50 && !target; k++) {
    await sleep(200);
    target = await fetch(`http://127.0.0.1:${port}/json`).then((r) => r.json()).then((l) => l.find((t) => t.type === 'page')).catch(() => null);
  }
  ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  ws.onmessage = (m) => {
    const d = JSON.parse(m.data);
    if (d.id && pending.has(d.id)) {
      const p = pending.get(d.id);
      pending.delete(d.id);
      d.error ? p.rej(new Error(d.error.message)) : p.res(d.result);
    }
  };
  await send('Page.enable');
  await send('Runtime.enable');
  const keys = async (list) => {
    for (const k of list) {
      await evalJs(`window.wfLauncherKey(${JSON.stringify(k)})`);
      await sleep(60);
    }
  };
  const run = async (label, list) => {
    await send('Page.navigate', { url: `${base}/app/?devrom=/rom.gba` });
    await until('window.wfLauncher === true', 120000, 'launcher');
    await sleep(500);
    if (label === 'battle') await shot('web-launcher.png');
    await keys(list);
    await until('window.wfStats && window.wfStats.frame > 120', 60000, `${label} running`);
    const st = await evalJs('JSON.stringify({frame: wfStats.frame, fps: wfStats.fps, view: wfStats.view, audio: wfStats.timings.audio})');
    console.log(label, st);
    await shot(`web-${label}.png`);
  };
  await run('battle', ['ArrowDown', 'Enter', 'ArrowRight', 'Enter']); // Battle > Fight: kind stays, cursor on START
  await run('story', ['Enter', 'Enter']); // Story > New Game
  console.log('web check OK');
} catch (e) {
  console.error('web check FAILED:', e.message);
  process.exitCode = 1;
} finally {
  chrome.kill();
}
