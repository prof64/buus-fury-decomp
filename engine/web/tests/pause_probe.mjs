// pause_probe.mjs OUT.png: PC wide (auto) in a 960x518 page, story Quick start, Start -> the pause menu; prints the
// picture's view / scale (playtest 42: the menu stayed 1x in a 480x259 view) and saves a screenshot
import { launch, serve, waitFor, sleep, tap } from './lib.mjs';
const out = process.argv[2] || '/tmp/pause_probe.png';
const { server, url } = await serve();
const { ctx } = await launch();
const page = await ctx.newPage();
await page.setViewportSize({ width: 960, height: 518 });
await page.addInitScript(() => localStorage.setItem('wf.prefs', JSON.stringify({ mode: 'pc:0', vol: 0 })));
await page.goto(`${url}/app/?devrom=/rom.gba`);
await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher' });
await page.close();
const p = await ctx.newPage();
await p.setViewportSize({ width: 960, height: 518 });
await p.goto(`${url}/app/?story=2-1&autostart`);
await waitFor(p, () => window.wf && window.wf.status().frame > 200, null, { what: 'story' });
await sleep(1500);
const field = await p.evaluate(() => window.wfStats && { view: window.wfStats.view, scale: window.wfStats.scale });
await tap(p, 'Enter', 60);
await sleep(1500);
const menu = await p.evaluate(() => window.wfStats && { view: window.wfStats.view, scale: window.wfStats.scale });
await p.screenshot({ path: out });
console.log(JSON.stringify({ field, menu }));
await ctx.close();
server.close();
