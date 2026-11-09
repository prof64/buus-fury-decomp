# Recovered local research

On November 8, 2026, seven local research lines were recovered before the old development checkout was retired.
The recovery and test-led integration were performed with Codex.
All original branch heads are ancestors of the corresponding `archive/2026-11-08/<name>` branches. These preserve
29 commits outside the previously published main branch. The archive notes identify historical or unfinished work.

| Research line | Preserved work | Status |
| --- | --- | --- |
| `audit-gate` | Compact opponent/skill recordings, frame hashes, regression gates and associated fixes | Integrated into main and tested |
| `coverage` | Optional coverage counters, reports, tools and 201 additional visual-comparison scenes | Integrated into main and tested; two later uncommitted report files also archived |
| `hud-text` | HUD drawing/timing and actor-shadow experiments | Archived; combined graphics changes did not pass the visual regression gates |
| `sprite-tail` | Emotes, sprite ordering, props, shadows and item icons | Archived; four later uncommitted working files also preserved |
| `worldmap-digits` | World-map labels, Dragon Radar and late map-object presentation | Archived |
| `boss-scenes` | Boss-scene shadows, script effects and rendering experiments | Archived |
| `accuracy` | World-map integration and unfinished boss-scene merge | Archived; ten uncommitted working files preserved, including conflict drafts |

The three archives with uncommitted work contain its exact files under `archive/uncommitted/` and a SHA-256
inventory in `archive/uncommitted.json`. Those files are outside the build's source tree. Conflict drafts are
historical recovery material, not resolutions to apply without review. The original committed source remains at
its original paths on each archive branch.

## Validation

With the original USA Buu's Fury ROM available at `roms/buu.gba`, from `engine/`:

```sh
cargo test --release -p wf-app --test audits --test pixels -- --test-threads=1
cargo test --release -p wf-core -p wf-render -p wf-import -p wf-view -p wf-app -- --test-threads=1
```

The compact audit gates compare 462 opponent scenarios / 215,555 frames and 538 skill scenarios / 180,160 frames.
315 opponent scenarios and 265 skill scenarios are exact throughout. Other scenarios retain explicitly recorded
limits on their known differences; the tests reject regressions beyond those limits. The pixel gate separately
locks 46,705 frames across 691 segments. Passing these checks does not establish accuracy for every possible input.

The broader suite has a pre-existing failure in `menus_match_emulator`: `link_trade_accept`, `link_trade_decline`,
`link_trade_declined`, `link_trade_propose`, `title_dojo` and `title_trade` retain one or two mismatching frames each.
The published baseline `e5c55b3` and the recovered version produce identical results for all 203 menu metric lines,
including these failures. The recovered code does not change those results.

ROM extraction caches, compiled binaries, raw screenshot/framebuffer dumps and old emulator saves were local
working material. They were not added to GitHub. The compact audit inputs, source, hashes and replay key logs
needed for the retained checks remain in Git. The historical whole-run measurements and their known exceptions
remain described in the main README and the existing replay notes.
