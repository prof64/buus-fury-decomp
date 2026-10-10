// replay_probe.mjs OUTDIR: a replay watched in the page like a player (headless, muted): a short battle recorded,
// played back; Start -> "Replay paused", A -> resume; the end -> "Replay finished"; mashing; Esc; B -> back to the
// launcher. Screenshots OUTDIR/rp_*.png, one JSON line of what happened.
import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { launch, serve, waitFor, sleep, tap, hold, status } from './lib.mjs';
const out = process.argv[2] || '/tmp/replay_probe';
mkdirSync(out, { recursive: true });
const { server, url } = await serve();
const { ctx } = await launch();
const res = {};
const p0 = await ctx.newPage();
await p0.goto(`${url}/app/?devrom=/rom.gba`);
await waitFor(p0, () => window.wfLauncher === true, null, { what: 'launcher' });
await p0.close();
const page = await ctx.newPage();
await page.goto(`${url}/app/`);
await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher' });
for (const k of ['ArrowDown', 'Enter', 'Enter']) {
  await page.evaluate((k) => window.wfLauncherKey(k), k);
  await sleep(150);
}
await waitFor(page, () => window.wf && window.wf.status().frame > 30, null, { what: 'battle' });
await hold(page, 'ArrowRight', 40);
await tap(page, 'KeyX', 10);
await sleep(2000);
// play the session's own recording back
const ok = await page.evaluate(() => {
  const x = window.wf.x;
  if (x.wf_app_current_replay() !== 0) return false;
  const b = new Uint8Array(x.memory.buffer, x.wf_blob_ptr(), x.wf_blob_len()).slice();
  const id = new TextEncoder().encode('probe');
  const p = x.wf_alloc(b.length), q = x.wf_alloc(id.length);
  new Uint8Array(x.memory.buffer, p, b.length).set(b);
  new Uint8Array(x.memory.buffer, q, id.length).set(id);
  return x.wf_app_play_replay(q, id.length, p, b.length, 0) === 0;
});
res.playing = ok && (await status(page)).mode === 'playback';
await sleep(500);
await tap(page, 'Enter', 5); // Start
await sleep(400);
await page.screenshot({ path: join(out, 'rp_paused.png') });
res.paused = (await status(page)).paused;
await tap(page, 'KeyX', 5); // A: Resume
await sleep(400);
res.resumed = !(await status(page)).paused;
// to the end: mash A / B / Start meanwhile
const t0 = Date.now();
while (Date.now() - t0 < 6000) {
  for (const k of ['KeyX', 'KeyZ', 'Enter']) await tap(page, k, 1);
}
await sleep(1500);
await page.screenshot({ path: join(out, 'rp_after_mash.png') });
const s1 = await status(page);
res.after_mash = { frame: s1.frame, end: s1.end, paused: s1.paused };
// stop mashing: the menu left up by the mashing takes a deliberate A (Resume)
await sleep(500);
if ((await status(page)).paused) await tap(page, 'KeyX', 5);
await sleep(300);
res.resumed_after_mash = !(await status(page)).paused;
// wait for the end (no buttons), then the finished menu
await waitFor(page, () => window.wf.status().paused && window.wf.status().frame >= window.wf.status().end, null, { what: 'replay end', timeout: 60000 }).catch(() => {});
await sleep(800);
await page.screenshot({ path: join(out, 'rp_finished.png') });
// B: Back -> the launcher
await tap(page, 'KeyZ', 5);
await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher again', timeout: 20000 }).then(() => (res.back_to_launcher = true)).catch(() => (res.back_to_launcher = false));
await page.screenshot({ path: join(out, 'rp_launcher.png') });
console.log(JSON.stringify(res));
await ctx.close();
server.close();
