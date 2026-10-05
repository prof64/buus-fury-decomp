// exploratory: ROM drop on a clean profile, launcher keys (argv[2], comma separated), then screenshots / status
//   node probe.mjs ArrowDown,Enter,ArrowUp,ArrowUp,Enter [wait-ms]
import { launch, serve, waitFor, sleep } from './lib.mjs';
const { server, url } = await serve();
const { ctx } = await launch();
const page = await ctx.newPage();
page.on('console', (m) => console.log('[page]', m.type(), m.text().slice(0, 300)));
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
await page.goto(`${url}/app/?devrom=/rom.gba`);
await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher' });
const keys = (process.argv[2] || '').split(',').filter(Boolean);
for (const [i, k] of keys.entries()) {
  if (i === keys.length - 1) await page.screenshot({ path: '/tmp/wt/web_probe_before.png' });
  await page.evaluate((k) => window.wfLauncherKey(k), k);
  await sleep(200);
}
await sleep(Number(process.argv[3] || 5000));
console.log(JSON.stringify(await page.evaluate(() => ({ last: window.wfLauncherLast?.choice, launcher: window.wfLauncher, status: window.wf && window.wf.status() }))));
await page.screenshot({ path: '/tmp/wt/web_probe_after.png' });
await ctx.close();
server.close();
