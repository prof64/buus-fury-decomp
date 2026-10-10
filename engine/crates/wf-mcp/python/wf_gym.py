"""Fast Python access to the deterministic Buu's Fury core (ctypes over libwf_mcp's C ABI; no build tools needed
beyond `cargo build --release -p wf-mcp`).

    from wf_gym import BuuEnv
    env = BuuEnv(preset="majin_fighters", level=60, frame_skip=4)
    obs = env.reset(seed=3)                      # numpy-free: a list of 124 ints (see OBS_DOC)
    obs, reward, done, info = env.step("RIGHT+A")  # gym-style; action = button string or int mask
    env.observe()                                # the full JSON observation the MCP tools return (dict)
    s = env.snapshot(); env.restore(s)           # cheap true snapshots (a World clone)
    env.hash()                                   # per-frame state hash (determinism checks)

    from wf_gym import call                      # the whole MCP tool API in-process
    call("new_battle", {"preset": "dabura"})

Env: WF_MCP_LIB (path to libwf_mcp.dylib/.so), WF_ROM / WF_CONTENT / WF_CACHE as for the server.
"""
import ctypes
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
_EXT = {"darwin": "dylib", "win32": "dll"}.get(sys.platform, "so")
_DEFAULT = os.path.normpath(os.path.join(HERE, "../../../target/release", ("" if sys.platform == "win32" else "lib") + "wf_mcp." + _EXT))

_lib = ctypes.CDLL(os.environ.get("WF_MCP_LIB", _DEFAULT))
_P = ctypes.c_void_p
_lib.wf_last_error.restype = ctypes.c_char_p
_lib.wf_gym_new.restype = _P
_lib.wf_gym_new.argtypes = [ctypes.c_char_p]
_lib.wf_gym_free.argtypes = [_P]
_lib.wf_gym_reset.argtypes = [_P, ctypes.c_int64]
_lib.wf_gym_step.argtypes = [_P, ctypes.c_uint16, ctypes.c_uint32]
_lib.wf_gym_step_many.argtypes = [_P, ctypes.POINTER(ctypes.c_uint16), ctypes.c_size_t]
_lib.wf_gym_step_slots.argtypes = [_P, ctypes.POINTER(ctypes.c_uint16), ctypes.c_size_t]
_lib.wf_gym_frame.argtypes = [_P]
_lib.wf_gym_frame.restype = ctypes.c_uint32
_lib.wf_gym_hash.argtypes = [_P]
_lib.wf_gym_hash.restype = ctypes.c_uint64
_lib.wf_gym_stats.argtypes = [_P, ctypes.c_uint32, ctypes.POINTER(ctypes.c_int64)]
_lib.wf_gym_obs_vec.argtypes = [_P, ctypes.c_uint32, ctypes.POINTER(ctypes.c_int32), ctypes.c_size_t]
_lib.wf_gym_obs_vec.restype = ctypes.c_size_t
_lib.wf_gym_observe.argtypes = [_P, ctypes.c_uint32, ctypes.c_uint32]
_lib.wf_gym_observe.restype = ctypes.c_char_p
_lib.wf_gym_snapshot.argtypes = [_P]
_lib.wf_gym_snapshot.restype = _P
_lib.wf_gym_restore.argtypes = [_P, _P]
_lib.wf_gym_snapshot_free.argtypes = [_P]
_lib.wf_gym_replay.argtypes = [_P]
_lib.wf_gym_replay.restype = ctypes.c_char_p
_lib.wf_call.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
_lib.wf_call.restype = ctypes.c_char_p

BUTTONS = {"A": 1, "B": 2, "SELECT": 4, "START": 8, "RIGHT": 16, "LEFT": 32, "UP": 64, "DOWN": 128, "R": 256, "L": 512}
OBS_LEN = 124
OBS_DOC = """[frame, outcome, x, y, facing, hp, hp_max, ep, ep_max, state, can_act, n_enemies,
 8 x enemy (id, dx, dy, hp, hp_max, melee, attacking, stunned), 8 x hostile shot (dx, dy, vx, vy, w, h)]
 melee: -1 out of reach, 0-3 = turn to that facing first, 4-7 = in range now (facing + 4);
 attacking: 0 no, 1 melee, 2 ranged, 3 grab, 4 hands; state: 0 idle 1 walk 2 dash 3 punch 4 block 5 hurt 6 dying 7 skill
 8 transform 9 held"""
STATS = ["frame", "outcome", "hp", "hp_max", "ep", "ep_max", "enemies_alive", "enemy_hp", "enemy_hp_max", "dealt_total", "taken_total", "kills"]


def mask(action):
    if isinstance(action, int):
        return action & 0x3FF
    m = 0
    for p in str(action).replace(",", "+").split("+"):
        p = p.strip().upper()
        if p:
            m |= BUTTONS[p]
    return m


def call(tool, args=None):
    """any MCP tool in-process (sessions live in this thread's server)"""
    r = json.loads(_lib.wf_call(tool.encode(), json.dumps(args or {}).encode()).decode())
    if isinstance(r, dict) and "error" in r and len(r) == 1:
        raise RuntimeError(r["error"])
    return r


class BuuEnv:
    """gym-style env over one battle. reward = damage dealt to primary enemies - hp_penalty * HP lost (+win bonus)."""

    def __init__(self, frame_skip=4, hp_penalty=1.0, win_bonus=0.0, record=False, **battle):
        battle = dict(battle)
        battle["record"] = record
        self.h = _lib.wf_gym_new(json.dumps(battle).encode())
        if not self.h:
            raise RuntimeError(_lib.wf_last_error().decode())
        self.frame_skip = frame_skip
        self.hp_penalty = hp_penalty
        self.win_bonus = win_bonus
        self._buf = (ctypes.c_int32 * OBS_LEN)()
        self._st = (ctypes.c_int64 * 12)()
        self._last = self.stats()

    def __del__(self):
        if getattr(self, "h", None):
            _lib.wf_gym_free(self.h)
            self.h = None

    def reset(self, seed=-1):
        if _lib.wf_gym_reset(self.h, seed) != 0:
            raise RuntimeError(_lib.wf_last_error().decode())
        self._last = self.stats()
        return self.obs()

    def obs(self, slot=0):
        _lib.wf_gym_obs_vec(self.h, slot, self._buf, OBS_LEN)
        return list(self._buf)

    def stats(self, slot=0):
        _lib.wf_gym_stats(self.h, slot, self._st)
        return dict(zip(STATS, self._st))

    def step(self, action, frames=None):
        out = _lib.wf_gym_step(self.h, mask(action), frames or self.frame_skip)
        st = self.stats()
        dealt = (self._last["enemy_hp"] - st["enemy_hp"]) if st["enemy_hp_max"] == self._last["enemy_hp_max"] else max(0, st["dealt_total"] - self._last["dealt_total"])
        lost = max(0, self._last["hp"] - st["hp"])
        reward = dealt - self.hp_penalty * lost + (self.win_bonus if out == 1 else 0.0)
        self._last = st
        return self.obs(), reward, out != 0, {"outcome": ["running", "won", "lost"][out], **st}

    def step_frames(self, masks):
        """one frame per mask (ints), fastest path"""
        arr = (ctypes.c_uint16 * len(masks))(*masks)
        return _lib.wf_gym_step_many(self.h, arr, len(masks))

    def observe(self, slot=0, full=False, map=False, forecast=True):
        return json.loads(_lib.wf_gym_observe(self.h, slot, (1 if full else 0) | (2 if map else 0) | (4 if forecast else 0)).decode())

    def frame(self):
        return _lib.wf_gym_frame(self.h)

    def hash(self):
        return "%016x" % _lib.wf_gym_hash(self.h)

    def snapshot(self):
        return _Snap(_lib.wf_gym_snapshot(self.h))

    def restore(self, s):
        _lib.wf_gym_restore(self.h, s.p)
        self._last = self.stats()

    def replay(self):
        return json.loads(_lib.wf_gym_replay(self.h).decode())


class _Snap:
    def __init__(self, p):
        self.p = p

    def __del__(self):
        if self.p:
            _lib.wf_gym_snapshot_free(self.p)
            self.p = None
