"""Validation play tests through the real MCP protocol (this client <-> wf-mcp over stdio).

Each fight is played twice: step by step (macro calls an LLM agent would make: observe -> decide -> act for 10-600
frames) and with a Rhai controller (run_controller); every finished session's replay is re-simulated with
play_replay and must reproduce the state hash. Then evaluate_controller runs the controllers over 32 seeds.

    python3 playtest.py            # prints a table, writes playtest_results.json next to this file
"""
import json
import os
import time

from mcp_client import MCP

HERE = os.path.dirname(os.path.abspath(__file__))
CTL = os.path.normpath(os.path.join(HERE, "../../../../agent/plugin/skills/buu-battle/controllers"))
m = MCP()
rows = []


def finish(kind, label, s, calls, t0):
    o = m.call("observe", session=s)
    rp = m.call("get_replay", session=s)
    chk = m.call("play_replay", replay=rp, keep=False)
    row = {
        "fight": label, "how": kind, "outcome": o["outcome"], "frames": o["frame"], "hp": o["player"]["hp"],
        "tool_calls": calls, "replay_hash": rp["hash"], "replay_ok": chk["deterministic"], "wall_s": round(time.time() - t0, 2),
    }
    rows.append(row)
    print(json.dumps(row))


# ---- step by step -------------------------------------------------------------------------------------------------

def melee_step_by_step(label, **battle):
    t0 = time.time()
    r = m.call("new_battle", **battle)
    s, o = r["session"], r["observation"]
    calls = 1
    while o["outcome"] == "running" and calls < 400:
        d = o.get("danger")
        hand = next((t for t in o.get("threats", []) if t["kind"] == "hand" and t.get("eta") == 0), None)
        if hand:
            # Janemba's hand is rising under us: step off it
            r = m.call("press", session=s, buttons="RIGHT" if o["player"]["pos"][0] > hand["pos"][0] else "LEFT", frames=14)
        elif d and d["hit_in"] <= 10 and not any(e.get("melee") == "in_range" for e in o["enemies"]) and d.get("if_blocking") == "no HP lost":
            r = m.call("block", session=s, frames=max(12, d["hit_in"] + 8))
        else:
            r = m.call("attack", session=s, stop_on=["hurt", "phase", "danger"], max_frames=600)
        calls += 1
        o = r["observation"]
    finish("step-by-step", label, s, calls, t0)


def yakon_step_by_step(**battle):
    """dodge/block the combo, beam him in the post-combo window (AI state 0x323)"""
    t0 = time.time()
    r = m.call("new_battle", **battle)
    s, o = r["session"], r["observation"]
    calls = 1
    while o["outcome"] == "running" and calls < 400:
        yak = next((e for e in o["enemies"] if e["name"] == "Yakon"), None)
        mini = next((e for e in o["enemies"] if e["name"] != "Yakon"), None)
        if mini and (yak is None or yak.get("state") == "0x325"):
            r = m.call("attack", session=s, target=str(mini["id"]), max_frames=300, stop_on=["hurt"])
        elif yak and yak.get("state") == "0x323" and "energy" in yak.get("takes", {}) and yak["takes"]["energy"].startswith("damage"):
            r = m.call("use_skill", session=s, skill="Kamehameha", target=str(yak["id"]), align=True, charge_frames=60)
        elif o.get("danger") and o["danger"]["hit_in"] <= 12:
            r = m.call("block", session=s, frames=o["danger"]["hit_in"] + 10)
        else:
            r = m.call("wait", session=s, frames=6, stop_on=["danger"])
        calls += 1
        o = r["observation"]
    finish("step-by-step", "Yakon (Goku Lv100, skill lv 2)", s, calls, t0)


# ---- controllers --------------------------------------------------------------------------------------------------

def controller(label, ctl, **battle):
    t0 = time.time()
    r = m.call("new_battle", **battle)
    s = r["session"]
    r = m.call("run_controller", session=s, code="@file:" + os.path.join(CTL, ctl), frames=12000)
    finish("controller " + ctl, label, s, 2, t0)


FIGHTS = [
    ("4x Majin Fighter (Goku Lv60)", "melee.rhai", dict(preset="majin_fighters", level=60)),
    ("6x Majin Fighter (Goku Lv70)", "melee.rhai", dict(enemy="t68", count=6, level=70, arena="forest")),
    ("Dabura (Goku Lv80)", "melee.rhai", dict(preset="dabura", level=80)),
    ("Janemba -> Super Janemba (Goku Lv110)", "melee.rhai", dict(preset="janemba", level=110)),
]
for label, ctl, b in FIGHTS:
    melee_step_by_step(label, **b)
    controller(label, ctl, **b)
yakon_step_by_step(preset="yakon", level=100, skill_level=2)
controller("Yakon (Goku Lv100, skill lv 2)", "yakon.rhai", preset="yakon", level=100, skill_level=2)
controller("Yakon (Goku Lv80, skill lv 1)", "yakon.rhai", preset="yakon", level=80, skill_level=1)

evals = []
for label, ctl, b in FIGHTS + [("Yakon (Goku Lv100, skill lv 2)", "yakon.rhai", dict(preset="yakon", level=100, skill_level=2)), ("Yakon (Goku Lv80, skill lv 1)", "yakon.rhai", dict(preset="yakon", level=80, skill_level=1))]:
    r = m.call("evaluate_controller", battle=b, code="@file:" + os.path.join(CTL, ctl), n_seeds=32, frames=12000)
    r.pop("per_seed")
    r["fight"], r["controller"] = label, ctl
    evals.append(r)
    print(json.dumps(r))

with open(os.path.join(HERE, "playtest_results.json"), "w") as f:
    json.dump({"plays": rows, "evaluations": evals}, f, indent=1)

print("\n| fight | how | result | frames | HP left | tool calls | replay hash ok |")
print("|---|---|---|---|---|---|---|")
for r in rows:
    print(f"| {r['fight']} | {r['how']} | {r['outcome']} | {r['frames']} | {r['hp'][0]}/{r['hp'][1]} | {r['tool_calls']} | {r['replay_ok']} |")
print("\n| fight | controller | seeds | win rate | mean HP lost | mean frames to win | wall ms |")
print("|---|---|---|---|---|---|---|")
for r in evals:
    print(f"| {r['fight']} | {r['controller']} | {r['runs']} | {r['win_rate']} | {r['mean_hp_lost']} | {r['mean_frames_to_win']} | {r['wall_ms']} |")
