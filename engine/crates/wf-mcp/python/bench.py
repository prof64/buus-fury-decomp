"""Steps/sec of the Python fast path (wf_gym) and determinism check.  python3 bench.py"""
import random
import time

from wf_gym import BuuEnv, call

R = {}


def timed(label, frames, fn):
    t = time.perf_counter()
    fn()
    dt = time.perf_counter() - t
    R[label] = frames / dt
    print(f"{label:58s} {frames / dt:12,.0f} frames/s")


env = BuuEnv(preset="majin_fighters", level=60, frame_skip=1)
rng = random.Random(1)
acts = [rng.choice([0, 1, 16, 32, 64, 128, 17, 33, 256]) for _ in range(20000)]

# 1. step_frames: one C call for a whole input list (no Python per frame)
env.reset(1)
timed("step_frames (batch, 20k frames)", 20000, lambda: env.step_frames(acts))

# 2. gym loop: step(mask) + obs vector + stats per decision, frame_skip 1 and 4
def loop(skip, n):
    env.reset(1)
    env.frame_skip = skip
    for i in range(n):
        o, r, d, info = env.step(acts[i % len(acts)])
        if d:
            env.reset(1)


timed("gym step + obs_vec + reward, frame_skip=1 (10k decisions)", 10000, lambda: loop(1, 10000))
timed("gym step + obs_vec + reward, frame_skip=4 (5k decisions)", 20000, lambda: loop(4, 5000))

# 3. full JSON observation each frame (what an LLM / controller sees, incl. 45-frame forecast)
env.reset(1)
env.frame_skip = 1


def jloop():
    for i in range(2000):
        env.step(acts[i])
        env.observe()


timed("step + JSON observe (with forecast) (2k)", 2000, jloop)

# 4. snapshots
env.reset(1)
env.step_frames(acts[:1000])
t = time.perf_counter()
for _ in range(20000):
    s = env.snapshot()
    env.restore(s)
dt = time.perf_counter() - t
print(f"{'snapshot + restore':58s} {dt / 20000 * 1e6:12.2f} us")

# determinism: same inputs -> same hash
env.reset(7)
env.step_frames(acts[:5000])
h1 = env.hash()
env.reset(7)
env.step_frames(acts[:5000])
print("deterministic:", h1 == env.hash(), h1)

# replay from the gym session verifies through the tool API
rp = env.replay()
r = call("play_replay", {"replay": rp, "keep": False})
print("replay via tool API:", r["deterministic"], r["hash"], f"{r['frames']} frames in {r['resim_ms']} ms")
