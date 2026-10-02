# Script runtime evaluation (mod language + decompile target)

**Recommendation: Rhai** (pure Rust, `no_float` + `only_i64` + `no_time`). Use it as the mod language and as
the target of the decompiler. Keep the exact bytecode interpreter in `wf-script` as the **verification
oracle**. It stays the reference that decompiled Rhai is checked against, trace for trace, and it can also
run original scripts directly.

Prototype and benchmark: `engine/experiments/lang-bench/`. It is a standalone cargo workspace and is not a
member of `engine/`.

## Constraints the runtime must meet

- **Rollback netcode** (GGRS). Every peer simulates. Late input restores a snapshot from up to ~8 frames ago
  and re-simulates. So the whole simulation state must be cheap to snapshot every frame, and it must be
  bit-identical on native and on wasm32-unknown-unknown.
- **Design assumed:**
  - Scripts are *run-to-completion event handlers*.
  - Anything that spans frames (wait, walk, dialog, transform) is an `Action` value: plain engine data
    such as `sequence([...])`, `parallel([...])`, or `then_call("handler", arg)` for continuations. The
    engine steps it.
  - A snapshot is a clone of engine data. No suspended script stack is ever saved.
  - The original VM matches this model. Its natives queue command objects for a director, or block on an
    engine-run loop, so the decompiler can emit timelines directly.
- **Integer-only API.**
  - No wall clock.
  - No nondeterministic iteration.
  - RNG only through the game PRNG (`0x0803FAAC`, reimplemented in `host`).
- **Multiplayer:** `player(n)` for n = 1..N. Per-player events are host calls.

## What was built

| piece | content |
|---|---|
| `host/` | Plain-data `World` (`Clone` = snapshot) with flags, G/P vars, stats, inventory, actors, players, timelines and trace. It also holds the `Action` enum, the timeline stepper, the game PRNG, `rand`/`chance`, the ~25 host functions, and the scenario driver. |
| `rt-rhai/` | Rhai binding. Actions are a registered Rust type. |
| `rt-boa/` | Boa (JavaScript) binding. Actions are plain JS objects built by `scripts/prelude.js` and walked by `run()`. It installs a fixed `Clock`. |
| `rt-mini/` | Candidate (d): a tiny stack VM whose whole state is `Clone`. Only `on_update` is hand-assembled. |
| `scripts/story.rhai`, `scripts/story.js` | Hand translations, in the timeline style, of #450 (Janemba → Super Janemba handover), #2 (equipment/item effects, abridged) and #497 (loot), plus a continuation handler and the per-actor `on_update`. Natives without names are `op(id, [...])` / `op_now(id, [...])`. |
| `scripts/api.d.ts` | TypeScript typings sketch for the JS option. |
| `bench/` | Native benchmark plus equivalence and rollback checks. |
| `wasm-rhai/`, `wasm-boa/`, `wasm-mini/` | Minimal wasm embeddings with plain `extern "C"` exports and no wasm-bindgen. |
| `runwasm.mjs` | Runs the wasm builds in node: imports, digests, timings. |
| `probes/rquickjs`, `probes/piccolo` | Feasibility probes. |

To reproduce:

```
cargo run --release -p bench
cargo build --target wasm32-unknown-unknown --profile release-wasm -p wasm-rhai -p wasm-boa -p wasm-mini
node runwasm.mjs
```

The Rhai size floor is built with `-p wasm-rhai --features slim`.

## Numbers

Measured on an Apple Silicon Mac, Rust 1.97, node 24. Rhai 1.26.1, Boa 0.22.0.

| | native Rust (baseline) | mini VM (d) | **Rhai** | Boa (JS) |
|---|---|---|---|---|
| 50 × `on_update` per frame, native | 0.52 µs (10 ns/call) | 2.5 µs (49 ns/call) | 47 µs (~930 ns/call); 45 µs with `OptimizationLevel::Full` | 30–32 µs (~620 ns/call) |
| wasm/native slowdown (same 300-frame run) | — | 2.3× (1.87 vs 0.81 ms) | 2.0× (29.9 vs 14.9 ms) | 3.0× (32.8 vs 11.0 ms) |
| ⇒ 50 handler calls per frame in wasm | — | ~6 µs | ~95 µs | ~95 µs |
| cold start: engine + compile 4 handlers | — | — | 23 µs + 100 µs | 120 µs + 600 µs |
| full rebuild (engine + compile) | — | — | 135 µs | 630–830 µs |
| wasm size, opt-level s + LTO (gzip -9) | — | 50 KB (21 KB) | 1.35 MB (331 KB); slim package set 1.12 MB (288 KB) | 4.10 MB (1.10 MB) |
| wasm imports needed | none | none | none | none, but only after a fixed `Clock` and `getrandom_backend="unsupported"` |
| runtime state between handlers (must be snapshotted) | — | none | **none**: AST is immutable, the Scope is rewound, and `fn`s cannot see script globals | the JS realm heap. It is mutable and not `Clone`. |
| engine snapshot (`World::clone`, bench world ~6.6 KB) | 1.8 µs | same | same | same |
| restore + replay 60 frames gives an identical digest | — | — | yes | yes, *only while scripts keep no JS-global state* |
| mid-execution snapshot | — | **yes** (VM is `Clone`; tested) | no (native recursion) | no |
| integer-only enforced | — | yes | **yes** (`no_float`: `7/2 == 3`; overflow is a deterministic error) | no (all numbers are f64; `7/2 == 3.5`; host must truncate) |

### Determinism checks

All passed:

- **Full scenario digest**, 600 frames: equal across Rhai and Boa, native and wasm (`009b8727675c07bd`).
  The scenario covers item use, three loot rolls, the handover timeline with its continuation, and per-frame
  updates.
- **Updates-only digest**, 300 frames: equal across native Rust, mini VM, Rhai and Boa, native and wasm
  (`9baaeba3335db407`).
- **Rollback:** snapshot at frame 60, run 60 frames, restore, run 60 frames again. The digest is identical
  for Rhai and Boa.

A hazard the bench demonstrates: a Boa script with `var counter` returned 1, then **3 after a restore**
(the correct value is 2). JS-global state lives outside the snapshot. The same Rhai script is rejected at
call time ("Variable not found"): Rhai `fn`s are pure, so persistent state *has to* go through host
variables that live in `World`.

### Candidates not prototyped

- **rquickjs (QuickJS, C):** the wasm32-unknown-unknown build fails. `cc` cannot find `stdlib.h`, because
  there is no wasm libc/sysroot without installing wasi-sdk. Excluded.
- **mlua (C Lua/Luau):** no wasm32-unknown-unknown support (it needs emscripten or wasi). It also cannot
  snapshot `lua_State`. Excluded.
- **piccolo (pure-Rust Lua 0.3.3):**
  - Runs natively (probe test passes).
  - Its wasm build needs a getrandom 0.2 shim (`rand` → `getrandom` "js" or custom). That is feasible.
  - `Lua`/`Executor` live in a `gc-arena`: they are not `Clone` and not serializable, so there is no
    snapshot of the runtime.
  - Pre-1.0, with a small ecosystem.
  - It has no advantage over Rhai under the run-to-completion design, and only offers Lua's familiarity.

## Rationale

1. **Rollback.** The run-to-completion + plain-data timeline design makes the runtime's own state
   irrelevant, *if the language cannot hide state*. Rhai guarantees this structurally: functions cannot
   capture or mutate script globals. JS cannot guarantee it without freezing the realm or rebuilding it
   per event (~0.7 ms), and Lua (piccolo) cannot either. A snapshot is then just `World::clone()`.
2. **Determinism.**
   - Rhai with `no_float` makes floats unrepresentable. Arithmetic is i64 with deterministic overflow
     errors. The original VM wraps at 32 bits, so the decompiler emits `wrap32()` where it matters.
   - With `no_time` there is no clock. Maps are `BTreeMap`. No imports are needed on wasm.
   - `set_max_operations` gives a deterministic hang guard: it counts operations, not time.
   - JS needs a clock hook, a getrandom stub and number discipline (every host call must truncate).
3. **Cost.**
   - Rhai's wasm binary is 3.6× smaller than Boa's (288–331 KB vs 1.1 MB gzipped).
   - Cold start is 5× faster, which matters for hot reload and desync recovery.
   - Per call, Rhai is ~1.5× slower than Boa natively but about equal in wasm (~95 µs for 50 calls).
   - Story scripts are events, not per-actor per-frame code. Even the worst case — 50 per-frame hooks
     re-simulated 8 frames during a rollback — is under 1 ms. Hot per-frame logic (AI, physics) stays in
     Rust.
4. **Modders.** JS/TS is the more familiar language, and `.d.ts` gives the best autocomplete. Rhai syntax
   is Rust/JS-like and small, and Rhai can emit API definition files (`metadata` feature,
   `Engine::definitions()`) for its language server. Familiarity does not outweigh points 1–3.
5. **The own VM (d)** is the fastest (18× Rhai) and the only runtime with mid-execution snapshots, at
   21 KB. But it means writing and maintaining a compiler, error messages and tooling. Its value is already
   captured where it is needed: the `wf-script` exact interpreter has exactly this shape, plain `Clone`
   state, and serves as the oracle (and can run the original bytecode with suspension at blocking natives).

## Decision

- **Mod language and decompile target: Rhai**, with `default-features = false` and features
  `std, no_float, no_time, no_module, no_custom_syntax, only_i64`.
  - One handler per event; every handler takes and returns ints.
  - Frame-spanning work is `run(sequence([...]))`; continuations are `then_call(name, arg)`.
  - Persistent state goes only through host vars and flags kept in the snapshot.
  - Optionally use the slim package set.
- **Decompiler rules found while translating:**
  - The original `and`/`or` are not short-circuit, so side-effecting natives (`chance`, `rand`) are hoisted
    into `let`s to keep the PRNG stream identical.
  - `loop_push n` runs the body n times (`for _ in 0..n`). With n = 0 it would wrap in the original, so the
    decompiler flags it.
  - Natives that return a value after blocking (dialog choices) split the script into `then_call`
    continuations.
  - `call` is a Rhai keyword, so the API uses `then_call`.
- **Verification oracle: the `wf-script` exact bytecode interpreter.** Run it and the decompiled Rhai
  against the same recording host and require identical traces, as this bench does across runtimes.
