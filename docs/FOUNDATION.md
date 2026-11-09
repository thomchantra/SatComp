# SatComp foundation: workflow, map and gotchas

Read this first. `HANDOFF.md` has the JSFX-era design decisions and history, `PARAMETERS.md` is the parameter
reference (the spec for both implementations), `TEST_CHECKLIST.md` is the manual test list.

## Status of the two builds (decided 2026-11-09)

**The Rust VST3/CLAP plug-in is the production build and the only one that gets new features.**
`SatComp.jsfx` is the **archival in-house prototype**: frozen at v0.3, kept as the reference for the DSP port and for
A/B and null tests. Do not add features to it and do not edit it unless the owner asks (e.g. to fix a reference bug).
This replaces the earlier "prototype in JSFX first, then port" loop; the steps below still describe how a parameter
or UI element is added, minus the JSFX step.

1. **Spec it** in `PARAMETERS.md` (what it does, AUDIT flags for anything odd).
2. **DSP** in `rust/satcomp-dsp` (pure, f64, no allocation or locks in the audio path). Unit tests with the change.
3. **Add the parameter**: a row in `rust/tools/gen_params.py` (slider number, id, range, default) and the field in
   `satcomp_dsp::Params`; map it in the generator's `to_dsp`. Run `python3 tools/gen_params.py` from `rust/`.
   Ids are permanent once released; slider numbers are the UI's addressing scheme (`sget(n)`).
4. **UI** in `rust/satcomp-plugin/src/gui/ui.rs`: `State::new` meta table (mode/snap/step), `fmt()` for the readout,
   then the widget in `frame()`. Same logical 560x336 grid.
5. **Validate**: `cargo test`, `cargo xtask bundle satcomp --release`, pluginval (below), then the owner tests in REAPER.
   Nothing counts as verified until they have.
6. **A bug that exists in both builds?** Fix it in Rust; mention the JSFX still has it unless asked to touch it.

Known intentional divergences (Rust vs the archival JSFX): internal smoothing on Drive / Bias / Makeup / Mix / Sat Mix
(`Engine.smoothing`, set false for null tests); Gain Match is on/off (Auto is retired); presets and the window
scale exist only in the plug-in; the **Custom waveshaper** (shaper 14, pencil draw view, `Curve` in `satcomp-dsp/src/shapers.rs`) is Rust-only, not in the JSFX; the sat level-match result is written by the UI thread (audio thread cannot set params).

## Colour schemes
`docs/colorthemesV2.svg` (+ `.png`) is the design source (dark swatches on the left, light on the right, 20 rows); `docs/colorscheme.md`
is its text export, now **edited by hand by the owner** (e.g. Sonar BG #000000) and the source of truth; `PALETTES` in `gui/canvas.rs` is `(name, dark, light)` generated from it by `tools/gen_palettes.py`. Row names came from the owner's layer list; the second
"Magenta" row (purple #240132) is called **Midnight** here (its pre-0.5 name) to keep names unique. **Check with the owner.**
Persisted as `palette-v2` (index into this table, default -1) + `light-mode` (bool). The legacy `palette` index (pre-0.5 table order) is
migrated once in `ui::initial_palette` (old White = Mono light; old Bananas/Apricot/Berry/Matcha were light-BG schemes). Only append new schemes.
Light mode: double-click the header icon (single click = next scheme, applied after a 0.3 s double-click window) or `MODE:` at the top of the
scheme menu (right-click the icon). Not verified in REAPER (clicks go through the host's mouse events).

## Git convention
- **Commit as you go, in small layers** so `git diff`/`git log -p` reads cleanly: one concern per commit, typically
  DSP -> params/presets -> UI -> docs/version. Each commit should build and pass `cargo test`; if a layer can't
  (e.g. DSP crate changes before the plug-in is adapted) say so in the message.
- Attempts that did not fix the problem still get their own labelled commit ("attempt, NOT fixed") so they are easy to revert.
- Messages: `area: what and why`, area = `dsp` / `plugin` / `gui` / `docs`. Commits end with the `Co-Authored-By` line.
- **Never rewrite or otherwise edit existing git state without asking the owner first**: no amend, rebase, reset,
  force-push, history edits, tag moves, config or remote changes. Adding new commits is fine without asking.

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
cargo xtask bundle-universal satcomp --release     # macOS arm64 + x86_64 (needs both rustup targets)
/Applications/pluginval.app/Contents/MacOS/pluginval --strictness-level 5 --validate target/bundled/satcomp.vst3
cargo build -p satcomp --features standalone --bin satcomp-standalone --release
target/release/satcomp-standalone --backend dummy  # opens the editor in a plain window
```
Screenshotting the standalone: list the window id (CGWindowListCopyWindowInfo, e.g. a small Swift script), then
`screencapture -x -o -l <id> out.png`. Capture the window only, never the whole screen.
Install for REAPER: copy the bundles to `~/Library/Audio/Plug-Ins/VST3/` and `.../CLAP/`.
`clap-validator` is not installed yet.

## Release (v0.4 freeze)
- macOS: universal VST3/CLAP built and pluginval-checked locally (ad-hoc signed by xtask, not notarized).
- Windows: cannot be built here. `.github/workflows/build.yml` builds macOS universal + Windows x64 on push, and on a
  `v*` tag zips both and attaches them to a GitHub release. **The Windows build and the first CI run are untested**;
  check them (and load the DLL bundle in a Windows DAW) before announcing it.
- Docs for users: `README.md`. `LICENSE` is GPL-3.0 (copied from the nih-plug checkout, the standard FSF text).

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

## Custom waveshaper (v0.4)
- Shaper 14 = a user curve: <=10 points (x=-1 and x=+1 fixed in x, y editable, up to 8 interior), monotone cubic
  (Fritsch-Carlson) so it never overshoots, flat beyond +-1 (= hard clip). Baked into the same 4097-point LUT as the
  other shapers, so drive/bias/oversampling behave identically.
- Stored as persisted `custom_curve` (not an automatable parameter) and as optional `curve` in preset files
  (absent = linear, so loading a preset always replaces the curve). Edit view: pencil icon in the sat plot; click empty =
  add + drag, drag = move, double-click = remove; view is drive-independent (unity input).
- Gotcha: parameter writes land in the host a frame or more later, so UI code must not read a parameter right after
  `click_set` and expect the new value (the draw view's auto-exit waits until it has seen shape 14).
- Not covered by tests: the mouse interaction (verified by screenshot only until tried in REAPER).

## Header, About page, preset marker (v0.4)
- Version: the header and About page read `CARGO_PKG_VERSION` (`version_short()` in `gui/ui.rs`), so bumping the crate
  version is the only edit. Both crates are 0.5.1.
- Clicking the title toggles the About page (`State.about`: 0 closed, 1 about, 2 changelog). It replaces the tab row, viz
  row and cells; the header, meter and right column stay live. Copy lives in `ABOUT_TEXT` / `CHANGELOG` (source text:
  `docs/abouttextcopy.md`). Changelog entries: newest first, one short sentence per bullet, no em dashes. About and Changelog text live in a row-based scroll box (`scroll_rows`: wheel = 3 rows, draggable thumb on the right, rows culled whole since the canvas has no clipping); scroll position resets on tab change.
- `tools/gen_palettes.py` (run from `rust/`) rewrites `PALETTES` from `docs/colorscheme.md`; edit the md, then run it.
- The signal-flow diagram is illustrative: dot size follows the live output level, and pre-compressor segments add the
  gain reduction back. There is no per-stage metering in the engine.
- Preset marker: the name is shown as `*NAME*` when any stored parameter or the custom curve differs from the loaded
  preset (`is_modified`, cached by name, 0.6 s settle after a load because host writes land late).
- VST3 vendor string is `thomchantra` (`lib.rs` `VENDOR`). REAPER caches the vendor from the first scan, so an older
  "(Trirat)" label needs Preferences > Plug-ins > VST > Clear cache / re-scan.

## Sidechain (v0.5)
- Spec in `PARAMETERS.md` (SIDECHAIN). Params 63-66 (`sc_on` is excluded from presets); layouts = stereo + aux "Sidechain",
  then plain stereo (`lib.rs`). Engine: `process_sc(l, r, Option<(&[f32], &[f32])>)`; `None` = no bus = internal detector.
  `tick_sc` / `sc_solo` / `mn_lv` in `satcomp-dsp/src/engine.rs`; tests `sidechain_*`.
- Solo (`Shared.sc_solo`) and the red-button hint (`Shared.sc_bus`, optimistic true until the first block) are UI atomics.
- **Unverified in a host:** how REAPER shows/routes the aux pins, pluginval with the aux layout, Windows. Nothing ran in REAPER yet.

## Open / next
- ~~Next session, idea 1: sidechain.~~ Done in 0.5 (see above); original brief: Add a second stereo input (nih-plug aux input) that feeds the detector, whole band,
  with HP/LP filters to taper it. Needs: audio layout change (main 2in/2out + aux 2in), an engine detector source switch,
  params (sidechain on, SC HP, SC LP) appended to `gen_params.py`, a UI home for them, and a listen/monitor option.
- **Next session, idea 2: SatXComp**, a 3-band multiband compressor with the saturator. Likely a separate repo that forks
  the `satcomp-dsp` crate, the egui canvas/UI toolkit, preset system and CI from here.
- Owner heard a click while drawing/switching shapers and when the plug-in window regains focus. Fixed what is
  identifiable (table morph on shape/curve change, no allocation in `Curve::from_points` on the audio thread);
  **the focus-regain click is unconfirmed**: retest, and if it persists look at the UI/host param traffic on focus.
- Null test against JSFX renders (REAPER offline render of sweep / noise / drum loop at fixed settings, compare with
  `smoothing = false`).
- Owner supplies hand-tuned presets for `rust/satcomp-plugin/presets/`.
- **Resize grip is still broken** (drag causes a stutter/scaling loop; two fix attempts, last = `80cd756`). Use the size dropdown (right-click grip). Revisit: commit-on-release with a ghost outline is the fallback design. File dialogs still need an owner test in REAPER. `clap-validator`, GitHub repo + CI (mac + Windows), signing/notarization.
- Not done: preset rename/delete, AU via clap-wrapper.
