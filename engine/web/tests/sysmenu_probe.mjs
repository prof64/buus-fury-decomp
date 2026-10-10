// sysmenu_probe.mjs OUTDIR: the PC system menu in the page like a player (headless, muted): the launcher by mouse
// (hover, click Battle, click GO), a fight, Esc -> the menu, the mouse over it (Settings, back), Enter, Quit to main menu
// -> the launcher. Screenshots OUTDIR/sm_*.png, one JSON line of what happened.
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { launch, serve, waitFor, sleep, tap, status } from './lib.mjs';
const out = process.argv[2] || '/tmp/sysmenu_probe';
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
await sleep(500);
// launcher pixels (240x160) -> page coordinates
const at = async (lx, ly) =>
  page.evaluate(([lx, ly]) => {
    const r = document.getElementById('screen').getBoundingClientRect();
    return [r.left + ((lx + 0.5) * r.width) / 240, r.top + ((ly + 0.5) * r.height) / 160];
  }, [lx, ly]);
await page.screenshot({ path: join(out, 'sm_launcher.png') });
// the main menu column: Story is the first row (y ~ 145 - rows * 10 + 2); hover then click "Battle" (row 2)
// (the page has no Quit row: Story, Battle, Online, Replays, Mods, Settings)
const rows = 6;
const top = 145 - (rows * 10 + 4) + 2;
let [mx, my] = await at(120, top + 10 + 4);
await page.mouse.move(mx, my);
await sleep(200);
await page.screenshot({ path: join(out, 'sm_launcher_hover.png') });
await page.mouse.click(mx, my);
await sleep(400);
await page.screenshot({ path: join(out, 'sm_battle_page.png') });
[mx, my] = await at(158, 78); // GO
await page.mouse.click(mx, my);
await waitFor(page, () => window.wf && window.wf.status().frame > 30, null, { what: 'fight started by mouse', timeout: 60000 })
  .then(() => (res.fight_by_mouse = true))
  .catch(() => (res.fight_by_mouse = false));
await sleep(500);
await page.screenshot({ path: join(out, 'sm_fight_hint.png') });
// Esc: the system menu
await tap(page, 'Escape', 10);
await sleep(300);
res.paused = (await status(page)).paused;
await page.screenshot({ path: join(out, 'sm_menu.png') });
// the mouse over the menu: the picture's view coordinates -> page coordinates (the menu is centred, scaled)
const view = await page.evaluate(() => {
  const c = document.getElementById('screen');
  const r = c.getBoundingClientRect();
  return { w: c.width, h: c.height, left: r.left, top: r.top, cw: r.width, ch: r.height };
});
const vat = (vx, vy) => [view.left + ((vx + 0.5) * view.cw) / view.w, view.top + ((vy + 0.5) * view.ch) / view.h];
const k = Math.max(1, Math.min(Math.floor(view.w / 240), Math.floor(view.h / 160)));
const ox = Math.floor((view.w - 240 * k) / 2), oy = Math.floor((view.h - 160 * k) / 2);
// sweep down the menu's middle column with the mouse: the highlight follows (screenshots of two spots)
for (const [i, my2] of [[1, 70], [2, 92]]) {
  const [px, py] = vat(ox + 120 * k, oy + my2 * k);
  await page.mouse.move(px, py);
  await sleep(150);
  await page.screenshot({ path: join(out, `sm_menu_hover${i}.png`) });
}
// Esc closes, the game runs again
await tap(page, 'Escape', 10);
await sleep(300);
res.resumed = !(await status(page)).paused;
// Esc, Up once (the last row: Quit to main menu), Enter -> the launcher
await tap(page, 'Escape', 10);
await tap(page, 'ArrowUp', 4);
await sleep(200);
await page.screenshot({ path: join(out, 'sm_menu_quit.png') });
await tap(page, 'Enter', 5);
await waitFor(page, () => window.wfLauncher === true, null, { what: 'launcher again', timeout: 30000 })
  .then(() => (res.quit_to_launcher = true))
  .catch(() => (res.quit_to_launcher = false));
await page.screenshot({ path: join(out, 'sm_back.png') });
console.log(JSON.stringify(res));
await ctx.close();
server.close();
