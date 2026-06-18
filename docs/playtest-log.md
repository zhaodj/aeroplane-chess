# Playtest Log

## 2026-06-18

### Automated Full-Match Simulation

Added a deterministic full-match simulation test that builds real match resources, spawns real piece state, repeatedly collects legal actions, executes turn resolution, and verifies that each scenario reaches `MatchResult.finished`.

Covered scenarios:
- 1v1, `SixOnly`, 1 piece, swapped Blue/Red seats.
- 1v1, `Even`, 4 pieces, Yellow/Green active seats.
- 2v2, `SixOnly`, 2 pieces, rotated seats while preserving P1+P3 vs P2+P4 teams.
- 2v2, `Even`, 4 pieces, mixed Human/AI controls.
- FFA, `SixOnly`, 1 piece, rotated seats.
- FFA, `Even`, 2 pieces, mixed Human/AI controls.

Findings:
- No gameplay blocker found in this automated pass.
- No fix was required from this pass; the new regression test remains in place for future changes.

### Expanded Full-Match Matrix

Expanded the deterministic full-match simulation from representative samples to a systematic matrix.

Matrix dimensions:
- Modes: `1v1`, `2v2`, `FFA`.
- Launch rules: `SixOnly`, `Even`.
- Pieces per player: `1`, `2`, `3`, `4`.
- Seat layouts: default, Blue/Red swapped, reversed active seat pairs, rotated seat order.
- Control layouts: alternating Human/AI and the opposite alternating layout.

Coverage:
- 192 generated match configurations.
- 3 additional representative edge configs for all-human 1v1, fast-mode 2v2, and single-piece FFA.

Findings:
- All generated configurations reached `MatchResult.finished`.
- Winner team and winner player ids were present for every completed match.
- No new gameplay issue was found in this pass.

### Browser Autoplay Smoke

Added a hidden browser smoke path for local wasm verification:
- URL flag: `?ac_autoplay=1v1-even-1`.
- The web shell cache-busts `aeroplane_chess.js` and `aeroplane_chess_bg.wasm` with the `cache` query value.
- The shell calls the exported `wasm_start()` after `wasm-bindgen` initialization.
- The app writes hidden DOM state for automation: `data-ac-smoke-state` and `data-ac-smoke-winners`.

Findings and fixes:
- Initial wasm page only loaded the canvas shell; the Rust app was not started. Fixed by exporting `wasm_start()` and invoking it from `web/index.html`.
- `scripts/build-wasm.sh` selected the bin wasm (`aeroplane-chess.wasm`) before the cdylib wasm (`aeroplane_chess.wasm`). Fixed the script to build `--lib` and prefer the cdylib artifact.
- Boot setup was registered on `OnEnter(AppState::Boot)`, which was not reliable for the initial web startup path. Fixed by moving boot initialization to the `Startup` schedule.
- Browser smoke passed on a fresh local origin: `state=ingame` then `state=result`, with `winners=1`.
