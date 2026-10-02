"""Drive the browser battle simulator (http://localhost:5317/#/battle) through chrome-devtools-axi for play-tests.

usage:
  play.py start [setup-json]        open the page, apply a setup (merged into localStorage 'bt-setup-v1'), press FIGHT
  play.py seq "R:30 R+B:5 A:1 -:20" [shot.png]
                                    stop the real-time loop and run frames synchronously with injected buttons
                                    (KEYS:FRAMES, keys joined by '+': A B SEL ST R L U D RB LB, '-' = none), render,
                                    print debug lines, optionally screenshot
  play.py js "expr"                 evaluate an expression in the page and print it
  play.py shot out.png              screenshot
"""
import json, os, subprocess, sys

ENV = dict(os.environ, CHROME_DEVTOOLS_AXI_SESSION='battle')
URL = 'http://localhost:5317/#/battle'


def axi(*args, stdin=None):
    r = subprocess.run(['chrome-devtools-axi', *args], input=stdin, capture_output=True, text=True, env=ENV, timeout=180)
    return (r.stdout + r.stderr).strip()


def run_js(script):
    return axi('run', stdin=script)


def page_eval(expr):
    return run_js('console.log(await page.eval(' + json.dumps(expr) + '));')


def main():
    cmd = sys.argv[1]
    if cmd == 'start':
        setup = json.loads(sys.argv[2]) if len(sys.argv) > 2 else {}
        axi('open', URL)
        axi('resize', '1500', '950')
        expr = f"""(() => {{
          const cur = JSON.parse(localStorage.getItem('bt-setup-v1') || '{{}}');
          const add = {json.dumps(setup)};
          const m = {{...cur, ...add, fighter: {{...(cur.fighter||{{}}), ...(add.fighter||{{}}), options: {{...((cur.fighter||{{}}).options||{{}}), ...((add.fighter||{{}}).options||{{}})}}, bonus: {{...((cur.fighter||{{}}).bonus||{{}}), ...((add.fighter||{{}}).bonus||{{}})}} }} }};
          localStorage.setItem('bt-setup-v1', JSON.stringify(m)); location.reload(); return 'ok' }})()"""
        print(page_eval(expr))
        print(run_js("await new Promise(r => setTimeout(r, 3500)); console.log(await page.eval(\"(() => { const b=[...document.querySelectorAll('button')].find(b => b.textContent.includes('FIGHT')); if(!b) return 'no fight button'; b.click(); return 'fight' })()\"));"))
        print(run_js("await new Promise(r => setTimeout(r, 1500)); console.log(await page.eval(\"(() => { const s = window.__battle; return s ? s.battle.debugLines(s.world).join('\\\\n') : 'no session' })()\"));"))
    elif cmd == 'seq':
        seq = []
        for tok in sys.argv[2].split():
            k, n = tok.rsplit(':', 1)
            seq.append([k, int(n)])
        expr = """(() => {
  const s = window.__battle; if (!s) return 'no session';
  s.loop.stop();
  const B = {A:1,B:2,SEL:4,ST:8,R:16,L:32,U:64,D:128,RB:256,LB:512};
  const seq = %s;
  for (const [keys, n] of seq) {
    let m = 0; for (const k of String(keys).split('+')) if (B[k]) m |= B[k];
    s.input.setInjected(m);
    for (let i = 0; i < n; i++) s.step();
  }
  s.input.setInjected(0);
  s.render();
  return s.battle.debugLines(s.world).join('\\n') + '\\n--- log\\n' + s.world.log.slice(-14).map(e=>e.frame+' '+e.text).join('\\n') + '\\noutcome ' + s.outcome;
})()""" % json.dumps(seq)
        print(page_eval(expr))
        if len(sys.argv) > 3:
            print(axi('screenshot', sys.argv[3]))
    elif cmd == 'js':
        print(page_eval(sys.argv[2]))
    elif cmd == 'shot':
        print(axi('screenshot', sys.argv[2]))
    elif cmd == 'console':
        print(axi('console', '--type', 'error'))


main()
