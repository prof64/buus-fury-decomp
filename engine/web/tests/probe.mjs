// exploratory: ROM drop on a clean profile, launcher -> Story -> Title Screen, then a scripted walk with screenshots
import { launch, serve, waitFor, sleep, tap, hold, status } from './lib.mjs';
const { server, url } = await serve();
const { ctx } = await launch();
const page = await ctx.newPage();
page.on('console', (m) => m.type() !== 'log' && console.log('[page]', m.type(), m.text().slice(0, 300)));
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
await page.goto(`${url}/app/?devrom=/rom.gba`);
await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher' });
await page.evaluate(() => window.wfLauncherKey('Enter'));
await sleep(300);
await page.evaluate(() => window.wfLauncherKey('Enter'));
await waitFor(page, () => window.wf && window.wf.status().frame > 0, null, { what: 'game running' });
const shot = async (n) => {
  const s = await status(page);
  console.log(n, 'frame', s.frame, JSON.stringify(await page.evaluate(() => window.wfStats && { hash: window.wfStats.state_hash, fps: window.wfStats.fps })));
  await page.screenshot({ path: `/tmp/wt/web_${n}.png` });
};
const steps = (process.argv[2] || 'w:800,s:a,Enter,w:60,s:b,Enter,w:60,s:c,Enter,w:60,s:d,Enter,w:120,s:e').split(',');
for (const st of steps) {
  const [k, v] = st.split(':');
  if (k === 'w') await sleep((Number(v) * 1000) / 59.7275);
  else if (k === 's') await shot(v);
  else if (k === 'h') await hold(page, v.split('*')[0], Number(v.split('*')[1]));
  else await tap(page, k);
}
await ctx.close();
server.close();
