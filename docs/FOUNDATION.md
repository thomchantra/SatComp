# SatComp foundation: workflow, map and gotchas

Read this first. `HANDOFF.md` has the JSFX-era design decisions and history, `PARAMETERS.md` is the parameter
reference (the spec for both implementations), `TEST_CHECKLIST.md` is the manual test list.

## The loop: prototype in JSFX, ship as VST3/CLAP

The JSFX is the **sketchbook** (fast to change, hot reload in REAPER). The Rust plug-in is **production**.
New ideas start in the JSFX; once the owner signs one off it is ported, never the other way round.

1. **Prototype in `SatComp.jsfx`.** Append new sliders at the end (keeps automation mapping), keep slider-dispatch
   chains short, run the case-insensitive-identifier check (see JSFX gotchas).
2. **Write it down in `PARAMETERS.md`** (what the code does today, AUDIT flags for anything odd). This is the port spec.
3. **Port the DSP** into `rust/satcomp-dsp` (pure, f64, no allocation in `tick`). Add unit tests with the port,
   not later. Keep the JSFX variable names where practical so the two read side by side.
4. **Add the parameter**: a row in `rust/tools/gen_params.py` (slider number, id, range, default) and the field in
   `satcomp_dsp::Params`; map it in the generator's `to_dsp`. Run `python3 tools/gen_params.py` from `rust/`.
   Ids are permanent once released; slider numbers are the UI's addressing scheme (`sget(n)`).
5. **Port the UI** in `rust/satcomp-plugin/src/gui/ui.rs`: `State::new` meta table (mode/snap/step = the JSFX `setp`),
   `fmt()` for the readout, then the widget in `frame()`. Same logical 560x336 grid as the JSFX.
6. **Validate**: `cargo test`, `cargo xtask bundle satcomp --release`, pluginval (below), then A/B against the JSFX
   in REAPER (load both on the same track, same settings). Nothing counts as verified until the owner has done that.
7. **Bug found in the VST first?** Fix it in the JSFX too if the JSFX is still the prototype of record, and note the
   divergence below.

Known intentional divergences (Rust vs JSFX): internal smoothing on Drive / Bias / Makeup / Mix / Sat Mix
(`Engine.smoothing`, set false for null tests); Gain Match is on/off (Auto is retired); presets and the window
scale exist only in the plug-in; the sat level-match result is written by the UI thread (audio thread cannot set params).

## Repo map

| Path | What |
|---|---|
| `SatComp.jsfx` | prototype, v0.3 |
| `docs/` | this file, HANDOFF, PARAMETERS, TEST_CHECKLIST, colorscheme, SatComp_spec |
| `proto/` | numpy reference scripts |
| `rust/satcomp-dsp` | DSP engine + tests. `Engine::begin_block(n)` = JSFX `@block`, `tick` = `@sample` |
| `rust/satcomp-plugin` | nih-plug shell (`lib.rs`), generated `params.rs`, `presets.rs`, `gui/{canvas,ui,mod}.rs`, `presets/*.json` (factory bank) |
| `rust/vendor/nih_plug_egui` | patched copy (see below) |
| `rust/xtask` | `cargo xtask bundle satcomp --release` |
| `rust/tools/gen_params.py` | generates `params.rs` |

## Commands (from `rust/`)

```
cargo test                                         # DSP + preset tests
cargo xtask bundle satcomp --release               # target/bundled/satcomp.vst3 + .clap
/Applications/pluginval.app/Contents/MacOS/pluginval --strictness-level 5 --validate target/bundled/satcomp.vst3
cargo build -p satcomp --features standalone --bin satcomp-standalone --release
target/release/satcomp-standalone --backend dummy  # opens the editor in a plain window
```
Screenshotting the standalone: list the window id (CGWindowListCopyWindowInfo, e.g. a small Swift script), then
`screencapture -x -o -l <id> out.png`. Capture the window only, never the whole screen.
Install for REAPER: copy the bundles to `~/Library/Audio/Plug-Ins/VST3/` and `.../CLAP/`.
`clap-validator` is not installed yet.

## Identity (never change after release)
VST3 class id `TrtSatCompPlugin`, CLAP id `com.trirat.satcomp`, vendor `thomchantra`, parameter ids in `params.rs`.
The JSFX `desc:` is also identity (still `SatComp`; ask before changing).

## Gotchas: JSFX / EEL2
- **Thread safety:** `@gfx` runs on its own thread and function params / `local()` vars are shared static storage.
  Audio code and UI code must not call the same function (UI copies are `u_*`, `gcomp_u`, `lutf_u`; audio-side EQ decode is `a_*`).
- **Identifiers are case-insensitive** (`eqR` == `eqr`); this once overwrote a pointer and caused +60 dB spikes.
  Scan for names differing only by case after every edit.
- Deep nested ternary chains break (26 deep left sliders unwritable): keep dispatch chains <= 10.
- No exponent literals (`1e-30`): write the decimals out.
- Sliders added to old projects can load as 0: only append new sliders, tell the owner to re-set defaults once.
- `%` truncates to int. `gfx_clear` colour = `r + g*256 + b*65536`.
- Do not change `desc:`. Edits that appear on disk are usually the owner tweaking `pick_font()`: re-read before editing.
- No JSFX compiler outside REAPER: nothing JSFX is verified until the owner loads it.

## Gotchas: Rust / nih-plug / egui
- **All parameter ranges must stay `Linear`.** The UI reads and writes plain values through `ParamPtr` normalized values
  (`min + (max-min)*norm`); a skewed range silently breaks every cell.
- `Engine::set_params` applies derived values immediately (latency must be valid before the first block).
  Keep `tick` allocation-free; the spectrum tap is a lock-free atomic ring (`TapRing`).
- Audio thread cannot set parameters: measurement results go through `Shared` atomics and the UI thread writes them
  (`sat_result_new`). Gain-match trim is persisted (`match_trim`), the palette and preset name too.
- Meters: audio thread `fetch_max`, UI `swap(0)`.
- **Vendored `nih_plug_egui`** has two patches: `EguiState::set_requested_size` is `pub` (aspect-locked resize), and
  `queue.resize` gets physical pixels (Retina resize bug). Re-apply if the crate is ever updated (`vendor/README.md`).
- egui draws text lower than REAPER does: `TEXT_LIFT_EM` (0.28) and `FONT_SCALE` (0.86) in `canvas.rs` were measured
  from side-by-side screenshots. Re-check them if the font or the JSFX sizes change.
- Layout is the JSFX grid (560x336 logical units, 25% scale steps); text and lines snap to device pixels.
  Default window 700x420 (125%), min 100%, max 250%, corner grip + scale button.
- Presets: parameter-id JSON; Bypass and Gain Match are never stored; factory bank = `presets/*.json` embedded at build;
  user files `*.satcomp` in `~/Library/Application Support/SatComp/Presets` (`%APPDATA%\SatComp\Presets` on Windows).
  File dialogs (`rfd`) block the UI and are untested inside REAPER (macOS) and on Windows.
- Dropbox syncs `rust/target/`: move it out (`CARGO_TARGET_DIR`) or mark it ignored.
- **Windows builds must come from a Windows machine** (GitHub Actions `windows-latest`); no cross-compile. Not set up yet.
- GPL is fine for this project; `nih_plug_egui` is ISC, Fira Code is OFL (keep the licence with any distribution).

## Open / next
- Null test against JSFX renders (REAPER offline render of sweep / noise / drum loop at fixed settings, compare with
  `smoothing = false`).
- Owner supplies hand-tuned presets for `rust/satcomp-plugin/presets/`.
- Resize grip and file dialogs need an owner test in REAPER. `clap-validator`, GitHub repo + CI (mac + Windows), signing/notarization.
- Not done: unsaved-changes marker, preset rename/delete, AU via clap-wrapper, `desc:`/version in the title.
